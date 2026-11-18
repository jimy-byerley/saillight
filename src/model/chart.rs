//! the object behind the main page: current folder, sectors, labels, hovered and selected
//! sector, details, progress of the scan and of the compression

use qmetaobject::{qt_base_class, qt_property, qt_signal, QObject, QString};

#[derive(QObject)]
pub struct ChartModel {
    base: qt_base_class!(trait QObject),
    /// folder the chart starts from
    folder: qt_property!(QString; NOTIFY folder_changed),
    folder_changed: qt_signal!(),
}

impl Default for ChartModel {
    fn default() -> Self {
        ChartModel {
            base: Default::default(),
            folder: home_folder().into(),
            folder_changed: Default::default(),
        }
    }
}

/// home folder of the user, `/` when it is not known
fn home_folder() -> String {
    std::env::var("HOME")
        .ok()
        .filter(|home| home.starts_with('/'))
        .unwrap_or_else(|| "/".to_owned())
}
