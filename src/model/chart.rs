//! the object behind the main page: current folder, sectors, labels, hovered and selected
//! sector, details, progress of the scan and of the compression.
//!
//! the logic is in `state.rs`; this file runs the long work on worker threads and gives
//! the results to QML as plain values

use std::path::{Path, PathBuf};

use qmetaobject::{
    qt_base_class, qt_method, qt_property, qt_signal, QObject, QPointer, QString, QVariant,
    QVariantList, QVariantMap,
};

use super::state::{Open, Release, State, DEFAULT_DEPTH, MAX_DEPTH};
use crate::actions::compress::{self, Failure, Format};
use crate::actions::delete;
use crate::chart::labels::Area;
use crate::chart::layout::{ring_radii, SectorKind};
use crate::config::{self, Config};
use crate::scan::tree::{Node, Tree};
use crate::scan::walk::{node_of, walk, Progress};
use crate::scan::{cache, mounts, stale};
use crate::worker::{self, Cancel};

#[allow(non_snake_case)]
#[derive(QObject)]
pub struct ChartModel {
    base: qt_base_class!(trait QObject),

    /// folder the chart is about
    folder: qt_property!(QString; NOTIFY folder_changed),
    folderName: qt_property!(QString; NOTIFY folder_changed),
    hasParent: qt_property!(bool; NOTIFY folder_changed),
    folder_changed: qt_signal!(),

    /// number of rings
    depth: qt_property!(i32; NOTIFY chart_changed),
    maxDepth: qt_property!(i32; CONST),
    /// whether the sizes of the folder are known
    known: qt_property!(bool; NOTIFY chart_changed),
    /// bytes used by the folder
    totalSize: qt_property!(f64; NOTIFY chart_changed),
    /// what to draw: `start`, `end` (turns from the top, clockwise), `inner`, `outer`
    /// (the chart radius being 1), `ring`, `group`, `kind` (0 item, 1 unreadable, 2 mount),
    /// `folder`
    sectors: qt_property!(QVariantList; NOTIFY chart_changed),
    chart_changed: qt_signal!(),

    /// names around the chart, in pixels from its centre: `sector`, `text`, `x`, `y`, `right`,
    /// `width`, and the arrow `ax`, `ay` to `ex`, `ey` to `tx`, `ty`
    labels: qt_property!(QVariantList; NOTIFY labels_changed),
    /// heights taken by the names above and below the chart, in portrait
    labelsAbove: qt_property!(f64; NOTIFY labels_changed),
    labelsBelow: qt_property!(f64; NOTIFY labels_changed),
    labels_changed: qt_signal!(),

    /// the storages of the device: `path`, `total`, `used`, `available` (bytes)
    mounts: qt_property!(QVariantList; NOTIFY mounts_changed),
    /// the storage the current folder is in: bytes it can hold and bytes taken, 0 when unknown
    volumeTotal: qt_property!(f64; NOTIFY mounts_changed),
    volumeUsed: qt_property!(f64; NOTIFY mounts_changed),
    mounts_changed: qt_signal!(),

    /// index of the sector under the finger, -1 for none
    hovered: qt_property!(i32; NOTIFY hover_changed),
    hover_changed: qt_signal!(),

    /// index of the selected sector, -1 for none
    selected: qt_property!(i32; NOTIFY selection_changed),
    /// about the selected sector, empty without one: `kind`, `name`, `path`, `size`, `date`
    /// (milliseconds), `permissions`, `owner`, `count`, `enterable`, `blocked`, `mounts`,
    /// `refusal`
    details: qt_property!(QVariantMap; NOTIFY selection_changed),
    selection_changed: qt_signal!(),

    scanning: qt_property!(bool; NOTIFY scan_changed),
    scanItems: qt_property!(f64; NOTIFY scan_changed),
    scanBytes: qt_property!(f64; NOTIFY scan_changed),
    scan_changed: qt_signal!(),

    /// the folder changed since its sizes were computed
    stale: qt_property!(bool; NOTIFY stale_changed),
    stale_changed: qt_signal!(),

    /// a deletion or compression is running
    busy: qt_property!(bool; NOTIFY busy_changed),
    busyText: qt_property!(QString; NOTIFY busy_changed),
    /// from 0 to 1, negative when it cannot be told
    busyProgress: qt_property!(f64; NOTIFY busy_changed),
    busyCancellable: qt_property!(bool; NOTIFY busy_changed),
    busy_changed: qt_signal!(),

    /// something went wrong, with what to tell the user
    failed: qt_signal!(message: QString),

    /// show the folder remembered from the last run. to call once the page exists
    start: qt_method!(fn(&mut self)),
    setFolder: qt_method!(fn(&mut self, path: QString)),
    goParent: qt_method!(fn(&mut self)),
    /// compute the sizes again
    refresh: qt_method!(fn(&mut self)),
    setDepth: qt_method!(fn(&mut self, depth: i32)),
    /// the room for the names, see `chart::labels::Area`
    setLabelArea: qt_method!(
        fn(&mut self, landscape: bool, radius: f64, line: f64, width: f64, height: f64, rows: i32)
    ),
    /// the finger is at (x, y) from the centre of the chart, its radius being 1
    hoverAt: qt_method!(fn(&mut self, x: f64, y: f64)),
    hoverNothing: qt_method!(fn(&mut self)),
    /// the finger left the screen there. true when it was on the chart
    releaseAt: qt_method!(fn(&mut self, x: f64, y: f64) -> bool),
    select: qt_method!(fn(&mut self, index: i32)),
    /// make the selected folder the current one
    enterSelected: qt_method!(fn(&mut self)),
    /// delete the item at `path`, which must be in the chart
    deleteItem: qt_method!(fn(&mut self, path: QString)),
    /// compress the item at `path` as `zip`, `gz` or `7z`, then delete it
    compressItem: qt_method!(fn(&mut self, path: QString, format: QString)),
    cancelBusy: qt_method!(fn(&mut self)),
    /// look whether the folder changed since its sizes were computed
    checkStale: qt_method!(fn(&mut self)),

    state: State,
    config: Config,
    cache_file: PathBuf,
    /// tests: run the work in place instead of on worker threads
    inline: bool,
    /// counts the folders shown, to drop the results of work started for a previous one
    generation: u64,
    scan: Option<Cancel>,
    check: Option<Cancel>,
    action: Option<Cancel>,
}

/// home folder of the user, `/` when it is not known
fn home_folder() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .unwrap_or_else(|| PathBuf::from("/"))
}

impl Default for ChartModel {
    fn default() -> Self {
        Self::new(home_folder(), Config::load(&config::default_path()), cache::default_path(), false)
    }
}

fn text(path: &Path) -> QString {
    QString::from(path.to_string_lossy().as_ref())
}

fn insert(map: &mut QVariantMap, key: &str, value: impl Into<QVariant>) {
    map.insert(QString::from(key), value.into());
}

fn strings(values: &[String]) -> QVariantList {
    values.iter().map(|value| QVariant::from(QString::from(value.as_str()))).collect()
}

/// the folder containing `path` as it is now, after the app changed something in it
fn folder_now(path: &Path) -> Option<(PathBuf, Node)> {
    let folder = path.parent()?;
    let metadata = std::fs::symlink_metadata(folder).ok()?;
    Some((folder.to_path_buf(), node_of("", &metadata)))
}

impl ChartModel {
    fn new(home: PathBuf, config: Config, cache_file: PathBuf, inline: bool) -> Self {
        let depth = config
            .get("chart", "depth")
            .and_then(|depth| depth.parse().ok())
            .unwrap_or(DEFAULT_DEPTH);
        // a remembered folder that is gone falls back to home
        let folder = config
            .get("chart", "folder")
            .map(PathBuf::from)
            .filter(|folder| folder.is_absolute() && folder.is_dir())
            .unwrap_or_else(|| home.clone());
        let state = State::new(home, folder, depth);
        let mut model = ChartModel {
            base: Default::default(),
            folder: Default::default(),
            folderName: Default::default(),
            hasParent: false,
            folder_changed: Default::default(),
            depth: state.depth as i32,
            maxDepth: MAX_DEPTH as i32,
            known: false,
            totalSize: 0.0,
            sectors: Default::default(),
            chart_changed: Default::default(),
            labels: Default::default(),
            labelsAbove: 0.0,
            labelsBelow: 0.0,
            labels_changed: Default::default(),
            mounts: Default::default(),
            volumeTotal: 0.0,
            volumeUsed: 0.0,
            mounts_changed: Default::default(),
            hovered: -1,
            hover_changed: Default::default(),
            selected: -1,
            details: Default::default(),
            selection_changed: Default::default(),
            scanning: false,
            scanItems: 0.0,
            scanBytes: 0.0,
            scan_changed: Default::default(),
            stale: false,
            stale_changed: Default::default(),
            busy: false,
            busyText: Default::default(),
            busyProgress: -1.0,
            busyCancellable: false,
            busy_changed: Default::default(),
            failed: Default::default(),
            start: Default::default(),
            setFolder: Default::default(),
            goParent: Default::default(),
            refresh: Default::default(),
            setDepth: Default::default(),
            setLabelArea: Default::default(),
            hoverAt: Default::default(),
            hoverNothing: Default::default(),
            releaseAt: Default::default(),
            select: Default::default(),
            enterSelected: Default::default(),
            deleteItem: Default::default(),
            compressItem: Default::default(),
            cancelBusy: Default::default(),
            checkStale: Default::default(),
            state,
            config,
            cache_file,
            inline,
            generation: 0,
            scan: None,
            check: None,
            action: None,
        };
        model.publish_folder();
        model
    }

    // ---- state to properties

    fn publish_folder(&mut self) {
        self.folder = text(&self.state.folder);
        self.folderName = match self.state.folder.file_name() {
            Some(name) => QString::from(name.to_string_lossy().as_ref()),
            None => QString::from("/"),
        };
        self.hasParent = self.state.parent().is_some();
        self.folder_changed();
    }

    fn publish_chart(&mut self) {
        let depth = self.state.depth;
        self.depth = depth as i32;
        self.known = self.state.tree.is_some();
        self.totalSize = self.state.total() as f64;
        self.sectors = self
            .state
            .sectors
            .iter()
            .map(|sector| {
                let (inner, outer) = ring_radii(sector.ring, depth);
                let mut map = QVariantMap::default();
                insert(&mut map, "start", sector.start);
                insert(&mut map, "end", sector.end);
                insert(&mut map, "inner", inner);
                insert(&mut map, "outer", outer);
                insert(&mut map, "ring", sector.ring as i32);
                insert(&mut map, "group", sector.group as i32);
                insert(
                    &mut map,
                    "kind",
                    match sector.kind {
                        SectorKind::Item => 0,
                        SectorKind::Unreadable => 1,
                        SectorKind::Mount => 2,
                    },
                );
                insert(&mut map, "folder", sector.folder);
                QVariant::from(map)
            })
            .collect();
        self.chart_changed();
        self.publish_hover();
        self.publish_selection();
        self.publish_mounts();
    }

    /// how full the storages are now
    fn publish_mounts(&mut self) {
        self.mounts = mounts::list()
            .iter()
            .map(|mount| {
                let mut map = QVariantMap::default();
                insert(&mut map, "path", text(&mount.path));
                insert(&mut map, "total", mount.total as f64);
                insert(&mut map, "used", mount.used as f64);
                insert(&mut map, "available", mount.available as f64);
                QVariant::from(map)
            })
            .collect();
        let (total, free, _) = mounts::usage(&self.state.folder).unwrap_or((0, 0, 0));
        self.volumeTotal = total as f64;
        self.volumeUsed = total.saturating_sub(free) as f64;
        self.mounts_changed();
    }

    fn publish_labels(&mut self) {
        let placement = &self.state.placement;
        self.labelsAbove = placement.above;
        self.labelsBelow = placement.below;
        self.labels = placement
            .labels
            .iter()
            .map(|label| {
                let name = self
                    .state
                    .tree
                    .as_ref()
                    .zip(self.state.sectors.get(label.sector))
                    .map(|(tree, sector)| tree.node(sector.node).name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let mut map = QVariantMap::default();
                insert(&mut map, "sector", label.sector as i32);
                insert(&mut map, "text", QString::from(name.as_str()));
                insert(&mut map, "x", label.x);
                insert(&mut map, "y", label.y);
                insert(&mut map, "right", label.right);
                insert(&mut map, "width", label.width);
                insert(&mut map, "ax", label.attach.0);
                insert(&mut map, "ay", label.attach.1);
                insert(&mut map, "ex", label.elbow.0);
                insert(&mut map, "ey", label.elbow.1);
                insert(&mut map, "tx", label.tip.0);
                insert(&mut map, "ty", label.tip.1);
                QVariant::from(map)
            })
            .collect();
        self.labels_changed();
    }

    fn publish_hover(&mut self) {
        let hovered = self.state.hovered.map_or(-1, |index| index as i32);
        if hovered != self.hovered {
            self.hovered = hovered;
            self.hover_changed();
        }
    }

    fn publish_selection(&mut self) {
        self.selected = self.state.selected.map_or(-1, |index| index as i32);
        let mut map = QVariantMap::default();
        if let Some(details) = self.state.details() {
            insert(&mut map, "kind", QString::from(details.kind));
            insert(&mut map, "name", QString::from(details.name.as_str()));
            insert(&mut map, "path", QString::from(details.path.as_str()));
            insert(&mut map, "size", details.size as f64);
            insert(&mut map, "date", details.modified as f64);
            insert(&mut map, "permissions", QString::from(details.permissions.as_str()));
            insert(&mut map, "owner", QString::from(details.owner.as_str()));
            insert(&mut map, "count", details.count as i32);
            insert(&mut map, "enterable", details.enterable);
            insert(&mut map, "blocked", strings(&details.blocked));
            insert(&mut map, "mounts", strings(&details.mounts));
            insert(&mut map, "refusal", QString::from(details.refusal.as_str()));
        }
        self.details = map;
        self.selection_changed();
        // the names are those of the selection
        self.publish_labels();
    }

    fn set_stale(&mut self, stale: bool) {
        if stale != self.stale {
            self.stale = stale;
            self.stale_changed();
        }
    }

    fn set_busy(&mut self, text: Option<String>, cancellable: bool) {
        self.busy = text.is_some();
        self.busyText = QString::from(text.unwrap_or_default().as_str());
        self.busyProgress = -1.0;
        self.busyCancellable = cancellable;
        self.busy_changed();
    }

    fn fail(&mut self, message: &str) {
        self.failed(QString::from(message));
    }

    // ---- work on other threads

    /// run `task` on a worker thread, then `done` on the Qt thread with its result
    fn spawn<T, F, D>(&mut self, task: F, done: D) -> Cancel
    where
        T: Send + 'static,
        F: FnOnce(&Cancel) -> T + Send + 'static,
        D: FnOnce(&mut Self, T) + 'static,
    {
        if self.inline {
            let cancel = Cancel::default();
            let result = task(&cancel);
            done(self, result);
            cancel
        } else {
            let this = QPointer::from(&*self);
            worker::spawn_to_qt(task, move |result| {
                // the model may have been destroyed meanwhile
                if let Some(this) = this.as_pinned() {
                    done(&mut this.borrow_mut(), result);
                }
            })
        }
    }

    /// a function for worker threads, calling `apply` on the Qt thread with what it is given
    fn reporter<T: Send + 'static>(
        &self,
        apply: impl Fn(&mut Self, T) + 'static,
    ) -> Box<dyn Fn(T) + Send + Sync> {
        if self.inline {
            return Box::new(|_| {});
        }
        let this = QPointer::from(self);
        Box::new(qmetaobject::queued_callback(move |value: T| {
            if let Some(this) = this.as_pinned() {
                apply(&mut this.borrow_mut(), value);
            }
        }))
    }

    fn stop_scan(&mut self) {
        if let Some(cancel) = self.scan.take() {
            cancel.cancel();
        }
        if let Some(cancel) = self.check.take() {
            cancel.cancel();
        }
    }

    /// get the sizes of the current folder, from the cache when allowed, and show them
    fn load(&mut self, use_cache: bool) {
        self.stop_scan();
        self.generation += 1;
        let generation = self.generation;
        self.scanning = true;
        self.scanItems = 0.0;
        self.scanBytes = 0.0;
        self.scan_changed();

        let folder = self.state.folder.clone();
        let cache_file = self.cache_file.clone();
        let report = self.reporter(move |this: &mut Self, progress: Progress| {
            if this.generation == generation && this.scanning {
                this.scanItems = progress.items as f64;
                this.scanBytes = progress.bytes as f64;
                this.scan_changed();
            }
        });
        let cancel = self.spawn(
            move |cancel| {
                if use_cache {
                    if let Some(tree) = cache::load(&cache_file, &folder) {
                        return Some((tree, true));
                    }
                }
                let tree = walk(&folder, |progress| report(progress), cancel)?;
                // the cache is only a convenience
                let _ = cache::save(&tree, &cache_file);
                Some((tree, false))
            },
            move |this, result: Option<(Tree, bool)>| {
                if this.generation != generation {
                    return;
                }
                this.scanning = false;
                this.scan = None;
                this.scan_changed();
                if let Some((tree, cached)) = result {
                    this.state.set_tree(tree);
                    this.publish_chart();
                    if cached {
                        this.check_stale();
                    } else {
                        this.set_stale(false);
                    }
                }
            },
        );
        if self.scanning {
            self.scan = Some(cancel);
        }
    }

    /// write the tree of the current folder to the cache, in the background
    fn save_cache(&mut self) {
        let Some(tree) = self.state.tree.clone() else { return };
        let cache_file = self.cache_file.clone();
        self.spawn(move |_| cache::save(&tree, &cache_file), |_, _| {});
    }

    fn check_stale(&mut self) {
        if self.scanning || self.busy {
            return;
        }
        let Some(tree) = self.state.tree.clone() else { return };
        if let Some(cancel) = self.check.take() {
            cancel.cancel();
        }
        let generation = self.generation;
        let cancel = self.spawn(
            move |cancel| stale::changed(&tree, &stale::own_folders(), cancel),
            move |this, changed| {
                if this.generation != generation {
                    return;
                }
                this.check = None;
                if let Some(changed) = changed {
                    this.set_stale(changed);
                }
            },
        );
        if !self.inline {
            self.check = Some(cancel);
        }
    }

    fn open(&mut self, folder: &Path) {
        if self.busy {
            return;
        }
        if folder == self.state.folder && (self.scanning || self.state.tree.is_some()) {
            return;
        }
        match self.state.set_folder(folder) {
            Open::Shown => {
                // what was running was about the previous folder
                self.stop_scan();
                self.generation += 1;
                if self.scanning {
                    self.scanning = false;
                    self.scan_changed();
                }
                self.publish_folder();
                self.publish_chart();
                self.save_cache();
            }
            Open::Scan => {
                self.publish_folder();
                self.publish_chart();
                self.set_stale(false);
                self.load(false);
            }
        }
        if let Some(folder) = self.state.folder.to_str() {
            self.config.set("chart", "folder", folder);
        }
    }

    // ---- called from QML

    fn start(&mut self) {
        if self.generation == 0 {
            self.publish_chart();
            self.load(true);
        }
    }

    #[allow(non_snake_case)]
    fn setFolder(&mut self, path: QString) {
        let path = PathBuf::from(path.to_string());
        if path.is_absolute() {
            self.open(&path);
        }
    }

    #[allow(non_snake_case)]
    fn goParent(&mut self) {
        if let Some(parent) = self.state.parent() {
            self.open(&parent);
        }
    }

    fn refresh(&mut self) {
        if self.busy {
            return;
        }
        cache::clear(&self.cache_file);
        // what is shown stays until the new sizes are there
        self.set_stale(false);
        self.load(false);
    }

    #[allow(non_snake_case)]
    fn setDepth(&mut self, depth: i32) {
        if self.state.set_depth(depth.max(1) as u32) {
            self.config.set("chart", "depth", &self.state.depth.to_string());
            self.publish_chart();
        }
    }

    #[allow(non_snake_case)]
    fn setLabelArea(&mut self, landscape: bool, radius: f64, line: f64, width: f64, height: f64, rows: i32) {
        if !(radius > 0.0 && line > 0.0) {
            return;
        }
        let area = Area { landscape, radius, line, width, height, rows: rows.max(0) as usize };
        if self.state.set_area(area) {
            self.publish_labels();
        }
    }

    #[allow(non_snake_case)]
    fn hoverAt(&mut self, x: f64, y: f64) {
        if self.state.hover(x, y) {
            self.publish_hover();
        }
    }

    #[allow(non_snake_case)]
    fn hoverNothing(&mut self) {
        if self.state.hover_nothing() {
            self.publish_hover();
        }
    }

    #[allow(non_snake_case)]
    fn releaseAt(&mut self, x: f64, y: f64) -> bool {
        let release = self.state.release(x, y);
        self.publish_hover();
        match release {
            Release::Parent => self.goParent(),
            Release::Selected | Release::Cleared => self.publish_selection(),
            Release::Nothing => return false,
        }
        true
    }

    fn select(&mut self, index: i32) {
        self.state.select(usize::try_from(index).ok());
        self.publish_selection();
    }

    #[allow(non_snake_case)]
    fn enterSelected(&mut self) {
        let Some((_, path)) = self.state.selected_item() else { return };
        if self.state.details().map_or(false, |details| details.enterable) {
            self.open(&path);
        }
    }

    /// the item at `path` when something can be done to it now
    fn target(&mut self, path: QString) -> Option<PathBuf> {
        if self.busy || self.scanning {
            return None;
        }
        let path = PathBuf::from(path.to_string());
        match self.state.removable(&path) {
            Ok(()) => Some(path),
            Err(message) => {
                self.fail(&message);
                None
            }
        }
    }

    fn name_of(path: &Path) -> String {
        path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    }

    #[allow(non_snake_case)]
    fn deleteItem(&mut self, path: QString) {
        let Some(path) = self.target(path) else { return };
        let name = Self::name_of(&path);
        self.set_busy(Some(format!("Deleting {name}")), false);
        let removed = path.clone();
        let cancel = self.spawn(
            move |_| {
                delete::remove(&removed).map_err(|error| error.to_string())?;
                Ok(folder_now(&removed))
            },
            move |this, result: Result<Option<(PathBuf, Node)>, String>| {
                this.action = None;
                this.set_busy(None, false);
                match result {
                    Ok(folder) => {
                        if let Some((folder, now)) = folder {
                            this.state.touched(&folder, &now);
                        }
                        this.state.removed(&path);
                        this.publish_chart();
                        this.save_cache();
                    }
                    Err(error) => {
                        // part of a folder may be gone
                        this.set_stale(true);
                        this.fail(&format!("{name} could not be deleted: {error}"));
                    }
                }
            },
        );
        if self.busy {
            self.action = Some(cancel);
        }
    }

    #[allow(non_snake_case)]
    fn compressItem(&mut self, path: QString, format: QString) {
        let Some(format) = Format::parse(&format.to_string()) else { return };
        let Some(path) = self.target(path) else { return };
        let name = Self::name_of(&path);
        self.set_busy(Some(format!("Compressing {name}")), true);
        let report = self.reporter(|this: &mut Self, share: f64| {
            if this.busy {
                this.busyProgress = share;
                this.busy_changed();
            }
        });
        let source = path.clone();
        let cancel = self.spawn(
            move |cancel| {
                let archive = compress::compress(&source, format, &*report, cancel)?;
                // describe the archive as the scan would
                let node = std::fs::symlink_metadata(&archive)
                    .map(|metadata| node_of(archive.file_name().unwrap_or_default(), &metadata))
                    .map_err(|error| Failure::Failed(error.to_string()))?;
                Ok((node, folder_now(&source)))
            },
            move |this, result: Result<(Node, Option<(PathBuf, Node)>), Failure>| {
                this.action = None;
                this.set_busy(None, false);
                match result {
                    Ok((archive, folder)) => {
                        if let Some((folder, now)) = folder {
                            this.state.touched(&folder, &now);
                        }
                        this.state.replaced(&path, archive);
                        this.publish_chart();
                        this.save_cache();
                    }
                    Err(Failure::Cancelled) => {}
                    Err(Failure::Failed(error)) => {
                        this.fail(&format!("{name} could not be compressed: {error}"));
                        // the archive may exist while the original could not be removed
                        this.check_stale();
                    }
                }
            },
        );
        if self.busy {
            self.action = Some(cancel);
        }
    }

    #[allow(non_snake_case)]
    fn cancelBusy(&mut self) {
        if let Some(cancel) = &self.action {
            cancel.cancel();
        }
    }

    #[allow(non_snake_case)]
    fn checkStale(&mut self) {
        self.publish_mounts();
        self.check_stale();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::layout::ring_radii;
    use crate::testing::TempDir;
    use std::fs;

    /// a model working in place on a folder of its own:
    ///
    /// ```text
    /// data/
    /// ├── big/
    /// │   ├── a       600 kB
    /// │   └── b       300 kB
    /// ├── small/
    /// │   └── c       60 kB
    /// └── d           40 kB
    /// ```
    struct Fixture {
        temp: TempDir,
        model: ChartModel,
    }

    fn write(path: &Path, kilobytes: usize) {
        // not compressible to nothing, so archives keep a visible size
        let data: Vec<u8> =
            (0..kilobytes as u64 * 1000).map(|index| ((index * 7919) % 251) as u8 ^ (index >> 9) as u8).collect();
        fs::write(path, data).unwrap();
    }

    fn fixture() -> Fixture {
        let temp = TempDir::new();
        let data = temp.path().join("data");
        fs::create_dir_all(data.join("big")).unwrap();
        fs::create_dir_all(data.join("small")).unwrap();
        write(&data.join("big/a"), 600);
        write(&data.join("big/b"), 300);
        write(&data.join("small/c"), 60);
        write(&data.join("d"), 40);
        let model = model_on(&temp, &data);
        Fixture { temp, model }
    }

    /// a model as a new run of the app would make, the current folder being `folder`
    fn model_on(temp: &TempDir, folder: &Path) -> ChartModel {
        let mut config = Config::load(&temp.path().join("config/config.ini"));
        config.set("chart", "folder", folder.to_str().unwrap());
        let mut model =
            ChartModel::new(temp.path().join("home"), config, temp.path().join("cache/tree"), true);
        model.start();
        model
    }

    impl Fixture {
        fn data(&self) -> PathBuf {
            self.temp.path().join("data")
        }

        /// tap in the middle of a ring, at an angle in turns
        fn tap(&mut self, ring: u32, turn: f64) -> bool {
            let (inner, outer) = ring_radii(ring, self.model.state.depth);
            let radius = (inner + outer) / 2.0;
            let angle = turn * std::f64::consts::TAU;
            self.model.releaseAt(radius * angle.sin(), -radius * angle.cos())
        }

        /// tap the sector of an item
        fn tap_item(&mut self, path: &str) {
            let path = self.data().join(path);
            let tree = self.model.state.tree.as_ref().unwrap();
            let node = tree.find(&path).unwrap();
            let sector = self
                .model
                .state
                .sectors
                .iter()
                .find(|sector| sector.node == node && !sector.is_marker())
                .unwrap_or_else(|| panic!("{} is not drawn", path.display()))
                .clone();
            assert!(self.tap(sector.ring, sector.middle()));
            assert_eq!(self.detail("path"), path.to_string_lossy());
        }

        fn delete_selected(&mut self) {
            let path = QString::from(self.detail("path").as_str());
            self.model.deleteItem(path);
        }

        fn compress_selected(&mut self, format: &str) {
            let path = QString::from(self.detail("path").as_str());
            self.model.compressItem(path, QString::from(format));
        }

        fn detail(&self, key: &str) -> String {
            self.model
                .details
                .into_iter()
                .find(|(name, _)| name.to_string() == key)
                .map(|(_, value)| value.to_qbytearray().to_string())
                .unwrap_or_default()
        }
    }

    #[test]
    fn starting_scans_the_remembered_folder() {
        let fixture = fixture();
        let model = &fixture.model;
        assert_eq!(model.folder.to_string(), fixture.data().to_string_lossy());
        assert_eq!(model.folderName.to_string(), "data");
        assert!(model.hasParent);
        assert!(model.known);
        assert!(!model.scanning);
        assert!(!model.stale);
        assert!(model.totalSize >= 1_000_000.0);
        // big, small, d, and a, b, c
        assert_eq!(model.sectors.len(), 6);
        assert_eq!(model.selected, -1);
        assert_eq!(model.hovered, -1);
        assert!(fixture.temp.path().join("cache/tree").exists());
    }

    #[test]
    fn a_folder_that_is_gone_falls_back_to_home() {
        let temp = TempDir::new();
        fs::create_dir(temp.path().join("home")).unwrap();
        fs::write(temp.path().join("home/file"), vec![1u8; 10_000]).unwrap();
        let model = model_on(&temp, &temp.path().join("missing"));
        assert_eq!(model.folder.to_string(), temp.path().join("home").to_string_lossy());
        assert_eq!(model.sectors.len(), 1);
    }

    #[test]
    fn tapping_selects_and_describes() {
        let mut fixture = fixture();
        fixture.tap_item("big");
        assert_eq!(fixture.detail("kind"), "folder");
        assert_eq!(fixture.detail("name"), "big");
        assert_eq!(fixture.detail("refusal"), "");
        assert!(fixture.model.selected >= 0);
        fixture.tap_item("big/a");
        assert_eq!(fixture.detail("kind"), "file");
        // outside of the chart nothing changes
        assert!(!fixture.model.releaseAt(3.0, 3.0));
        assert_eq!(fixture.detail("name"), "a");
    }

    #[test]
    fn hovering_is_published() {
        let mut fixture = fixture();
        let (inner, outer) = ring_radii(1, 3);
        fixture.model.hoverAt(0.0, (inner + outer) / 2.0);
        assert!(fixture.model.hovered >= 0);
        fixture.model.hoverNothing();
        assert_eq!(fixture.model.hovered, -1);
    }

    #[test]
    fn entering_a_folder_and_coming_back() {
        let mut fixture = fixture();
        fixture.tap_item("big");
        fixture.model.enterSelected();
        assert_eq!(fixture.model.folderName.to_string(), "big");
        assert_eq!(fixture.model.sectors.len(), 2);
        assert_eq!(fixture.model.selected, -1);
        // the centre goes to the parent, which is scanned again
        assert!(fixture.model.releaseAt(0.0, 0.0));
        assert_eq!(fixture.model.folderName.to_string(), "data");
        assert_eq!(fixture.model.sectors.len(), 6);
        // a file cannot be entered
        fixture.tap_item("d");
        fixture.model.enterSelected();
        assert_eq!(fixture.model.folderName.to_string(), "data");
        // a path given by the path bar
        fixture.model.setFolder(text(&fixture.data().join("small")));
        assert_eq!(fixture.model.folderName.to_string(), "small");
        assert_eq!(fixture.model.sectors.len(), 1);
        fixture.model.setFolder(QString::from("not/absolute"));
        assert_eq!(fixture.model.folderName.to_string(), "small");
    }

    #[test]
    fn the_folder_and_depth_are_remembered() {
        let mut fixture = fixture();
        fixture.model.setDepth(1);
        assert_eq!(fixture.model.sectors.len(), 3);
        fixture.model.setFolder(text(&fixture.data().join("big")));

        let config = Config::load(&fixture.temp.path().join("config/config.ini"));
        assert_eq!(config.get("chart", "depth"), Some("1"));
        assert_eq!(config.get("chart", "folder"), fixture.data().join("big").to_str());
        let model = ChartModel::new(
            fixture.temp.path().join("home"),
            config,
            fixture.temp.path().join("cache/tree"),
            true,
        );
        assert_eq!(model.depth, 1);
        assert_eq!(model.folderName.to_string(), "big");
    }

    #[test]
    fn the_next_run_starts_from_the_cache() {
        let fixture = fixture();
        let data = fixture.data();
        // what the cache does not know is not shown, which proves the cache was used
        write(&data.join("later"), 500);
        let model = model_on(&fixture.temp, &data);
        assert_eq!(model.sectors.len(), 6);
        // and the folder is seen as changed
        assert!(model.stale);
    }

    #[test]
    fn an_unchanged_folder_is_not_stale() {
        let fixture = fixture();
        let model = model_on(&fixture.temp, &fixture.data());
        assert_eq!(model.sectors.len(), 6);
        assert!(!model.stale);
    }

    #[test]
    fn refresh_computes_again() {
        let mut fixture = fixture();
        write(&fixture.data().join("later"), 500);
        fixture.model.checkStale();
        assert!(fixture.model.stale);
        fixture.model.refresh();
        assert!(!fixture.model.stale);
        assert_eq!(fixture.model.sectors.len(), 7);
        // the cache follows
        let model = model_on(&fixture.temp, &fixture.data());
        assert_eq!(model.sectors.len(), 7);
        assert!(!model.stale);
    }

    #[test]
    fn deleting_removes_the_item_and_updates_the_chart() {
        let mut fixture = fixture();
        let before = fixture.model.totalSize;
        fixture.tap_item("big/a");
        fixture.delete_selected();
        assert!(!fixture.data().join("big/a").exists());
        assert!(fixture.data().join("big/b").exists());
        assert!(!fixture.model.busy);
        assert_eq!(fixture.model.selected, -1);
        assert!(fixture.model.totalSize < before - 500_000.0);
        assert_eq!(fixture.model.sectors.len(), 5);
        // the tree still matches the folder
        fixture.model.checkStale();
        assert!(!fixture.model.stale);
        // and the cache was rewritten
        let model = model_on(&fixture.temp, &fixture.data());
        assert_eq!(model.sectors.len(), 5);
        assert!(!model.stale);
    }

    #[test]
    fn deleting_a_folder() {
        let mut fixture = fixture();
        fixture.tap_item("big");
        fixture.delete_selected();
        assert!(!fixture.data().join("big").exists());
        assert_eq!(fixture.model.sectors.len(), 3);
    }

    #[test]
    fn nothing_selected_nothing_deleted() {
        let mut fixture = fixture();
        fixture.delete_selected();
        fixture.compress_selected("zip");
        assert_eq!(fixture.model.sectors.len(), 6);
        assert!(fixture.data().join("big/a").exists());
    }

    #[test]
    fn only_what_the_chart_knows_is_deleted() {
        let mut fixture = fixture();
        // made after the scan, and outside of the folder
        write(&fixture.data().join("later"), 10);
        write(&fixture.temp.path().join("outside"), 10);
        for path in [fixture.data().join("later"), fixture.temp.path().join("outside"), fixture.data()] {
            fixture.model.deleteItem(text(&path));
            fixture.model.compressItem(text(&path), QString::from("zip"));
            assert!(path.exists(), "{}", path.display());
        }
        fixture.model.deleteItem(QString::from(""));
        assert_eq!(fixture.model.sectors.len(), 6);
    }

    #[test]
    fn the_home_folder_is_never_deleted() {
        let temp = TempDir::new();
        let home = temp.path().join("home");
        fs::create_dir_all(home.join("docs")).unwrap();
        write(&home.join("docs/file"), 100);
        fs::create_dir_all(temp.path().join("other")).unwrap();
        write(&temp.path().join("other/file"), 100);
        // the chart is about the folder containing home
        let mut fixture = Fixture { model: model_on(&temp, temp.path()), temp };
        let home_node = fixture.model.state.tree.as_ref().unwrap().find(&home).unwrap();
        let sector = fixture
            .model
            .state
            .sectors
            .iter()
            .find(|sector| sector.node == home_node)
            .unwrap()
            .clone();
        fixture.tap(sector.ring, sector.middle());
        assert_eq!(fixture.detail("name"), "home");
        assert_ne!(fixture.detail("refusal"), "");
        fixture.delete_selected();
        fixture.compress_selected("zip");
        assert!(home.join("docs/file").exists());
    }

    #[test]
    fn compressing_replaces_the_item_by_its_archive() {
        for (format, archive) in [("zip", "big.zip"), ("gz", "big.tar.gz"), ("7z", "big.tar.7z")] {
            let mut fixture = fixture();
            let before = fixture.model.totalSize;
            fixture.tap_item("big");
            fixture.compress_selected(format);
            assert!(!fixture.data().join("big").exists(), "{format}");
            assert!(fixture.data().join(archive).exists(), "{format}");
            assert!(!fixture.model.busy);
            assert!(fixture.model.totalSize < before, "{format}");
            // the archive is selected in place of the folder
            assert_eq!(fixture.detail("name"), archive, "{format}");
            assert_eq!(fixture.detail("kind"), "file", "{format}");
            fixture.model.checkStale();
            assert!(!fixture.model.stale, "{format}");
        }
    }

    #[test]
    fn compressing_a_file() {
        let mut fixture = fixture();
        fixture.tap_item("big/a");
        fixture.compress_selected("gz");
        assert!(fixture.data().join("big/a.gz").exists());
        assert!(!fixture.data().join("big/a").exists());
        fixture.compress_selected("rar");
        assert!(fixture.data().join("big/a.gz").exists());
    }

    #[test]
    fn labels_are_published_for_the_area() {
        let mut fixture = fixture();
        assert_eq!(fixture.model.labels.len(), 0);
        fixture.model.setLabelArea(false, 300.0, 40.0, 340.0, 0.0, 5);
        assert_eq!(fixture.model.labels.len(), 3);
        assert!(fixture.model.labelsAbove > 0.0 || fixture.model.labelsBelow > 0.0);
        // a senseless area is ignored
        fixture.model.setLabelArea(false, 0.0, 40.0, 340.0, 0.0, 5);
        assert_eq!(fixture.model.labels.len(), 3);
        fixture.model.setLabelArea(true, 250.0, 40.0, 200.0, 300.0, 0);
        assert_eq!(fixture.model.labels.len(), 3);
        assert_eq!(fixture.model.labelsAbove, 0.0);
    }

    #[test]
    fn names_follow_the_selection() {
        let mut fixture = fixture();
        fixture.model.setLabelArea(false, 300.0, 40.0, 340.0, 0.0, 5);
        let above = fixture.model.labelsAbove;
        fixture.tap_item("big");
        assert_eq!(fixture.model.labels.len(), 2);
        // the chart does not move
        assert_eq!(fixture.model.labelsAbove, above);
        fixture.model.select(-1);
        assert_eq!(fixture.model.labels.len(), 3);
    }

    #[test]
    fn storages_are_published() {
        let mut fixture = fixture();
        fixture.model.checkStale();
        assert_eq!(fixture.model.mounts.len(), mounts::list().len());
        // the storage of the current folder. not known for a temporary folder in the build
        // target, where paths are remapped for some system calls only
        assert!(fixture.model.volumeTotal >= fixture.model.volumeUsed);
    }
}
