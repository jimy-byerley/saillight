# implementation choices

choices marked *(to confirm)* were made during the unattended implementation and are listed in the journal

## sizes

- the size of an item is the space it uses on disk (its blocks), not its length. a folder counts its own blocks plus everything inside
- a file with several names (hard links) is counted once, under the first name met; its other names weigh nothing
- symbolic links are never followed: a link weighs what the link itself uses
- a folder on another filesystem than the current folder is a mount point: it is not entered and weighs nothing

## scan

- `src/scan/` knows nothing about the chart, `src/chart/` nothing about the filesystem. the size tree (`scan/tree.rs`) is what goes from one to the other, and what the cache stores
- the tree keeps every item, however small
- the scan reports the number of items and bytes counted so far, at most every 100 ms. it cannot tell how far it is, so its progress bar has no end
- entering a folder of the chart reuses what is known of it, without a scan. going to a parent folder scans it *(to confirm)*

## cache

- one file, `~/.cache/saillight/tree`, holding the tree of the current folder only. written after each scan, each change of current folder and each deletion or compression; dropped by refresh
- text format with a version line and a closing count, so a truncated or foreign file is ignored
- at start the cached tree is shown at once, then compared to the folder in the background

## changed folder warning

- the cached tree is compared to the filesystem: every folder by its modification date, every other item by its date, kind and size. the first difference lights the warning
- done when a cached tree is loaded and each time the app comes back in front
- the app's own files (`~/.cache/saillight`, `~/.config/saillight`) are not looked at
- after a deletion or compression made by the app, the tree takes the new date of the folder it happened in, so the app's own actions do not light the warning
- a tap on the warning refreshes *(to confirm)*

## chart

- geometry is computed in rust, without Qt (`src/chart/`): sectors, the sector under a touch point, positions of the names
- QML only draws: `Chart.qml` paints the sectors on a `Canvas` with its native arcs. the hovered and selected sectors are painted on a second canvas above, so sliding does not repaint all the sectors
- the full chart is repainted only when its data changes
- canvases are only given colors as text (`Logic.cssColor`), and their state is reset at each painting, see the journal
- items are placed biggest first, clockwise from the top. what is under 1% of the full circle is not drawn; the gap is at the end of its folder's sector
- colors: one hue per item of the current folder, shared by what is inside it; rings get darker outward; files are less saturated than folders *(to confirm)*
- the centre shows the current folder and its size
- fallback if the canvas is too slow on a phone: a painted item written in the C++ embedded with `cpp!`, drawing the same sectors. only `Chart.qml` would change

## markers

- a folder holding something that could not be read gets a zero-width sector at the end of its own, in the ring of its content, drawn as a thick red line. every folder on the way down to the problem has one, so it can be followed ring by ring *(to confirm)*
- a folder holding mount points and nothing unreadable gets the same marker in another color
- selecting a marker lists the unreadable paths and the mount points under its folder. a tap on a mount point makes it the current folder

## interaction

- a press on the chart highlights the sector under the finger, sliding moves the highlight, releasing selects. releasing out of the chart changes nothing, releasing on a gap clears the selection
- the chart keeps the touches that start on its disc, so the page cannot be scrolled or its menu pulled from there; the corners around the disc, the names, the path and the details can *(to confirm)*
- pull-down menu: refresh, more levels, fewer levels (1 to 6)
- a tap on a name selects its sector

## names around the chart

- only the items of the current folder are named
- portrait: one name per line above and below the chart, at most 5 lines each, the biggest items first. names of the sectors closest to the sides are the furthest from the chart, so their arrows go around the others. a name on the left half starts at its arrow, a name on the right half ends at it, and stops before the arrows crossing its line
- landscape: a column of names on each side, at the height of their sector when there is room. the chart and its names take 2/3 of the width, the details the rest
- an arrow goes from the name to the rim of the chart, then straight into the sector of the first ring, ending with a dot

## deletion

- asked through the usual Silica remorse countdown, and bound to the path that was shown when it was asked, not to what is selected when it ends
- only what the chart knows can be deleted
- refused whatever the permissions: `/`, the home folder and the folders it is in, the current folder itself, and a folder containing a mount point (its content is another storage) *(to confirm)*
- a link is removed, never its target
- the chart is updated in place, without a scan

## compression

- the archive is written next to the original as `name.zip`, `name.gz` / `name.tar.gz`, `name.7z` / `name.tar.7z`; a taken name gets a number (`name (2).zip`)
- written under a hidden temporary name (`.name.zip.part`), read back entirely and compared to what was written (checksums), renamed, and only then the original is deleted. a failure or a cancellation before that leaves the original untouched and no archive
- no intermediate tar file: the tar is streamed into the compressor
- tar keeps links as links, permissions and dates. zip keeps links, permissions and dates too, but refuses names that are not valid text; so does 7z for the name of the item
- 7z on a 32-bit device is refused above 3.5 GB: the library counts sizes in machine words
- progress counts the bytes read from the original, then the bytes read back from the archive; it can be cancelled
- only files and folders can be compressed, and the same places are refused as for deletion
- crates: `flate2`, `tar`, `zip` 0.6, `sevenz-rust` 0.6 (lzma2), `crc32fast`

## background work

- no async runtime: scan, cache writing, comparison, deletion and compression each run on a plain thread (`worker.rs`), with a cancellation flag checked between two steps of the work. results come back to the Qt thread with `qmetaobject::queued_callback`
- a counter of the folders shown drops the results of work started for a previous folder

## QML side

- a single object is exposed to QML, `ChartModel` (module `Saillight 1.0`). its logic is in `model/state.rs`, without Qt; `model/chart.rs` runs the work and gives the results as plain values
- components take the model as a property, so tests give them a stand-in

## settings

- `~/.config/saillight/config.ini`: number of rings and current folder, written at each change
- the app starts in the folder it was in, the home folder the first time or when that folder is gone

## repository structure

guideline for where new files go, not an exhaustive list

```
saillight/
├── Cargo.toml                single crate, no workspace
├── build.rs                  compiles the C++ shim (cpp crate) for SailfishApp::application()
├── Cargo.lock                committed, crate versions building with the SDK's rust
├── .cargo/config.toml        cargo settings (rust-version aware resolution)
├── scripts/                  development helpers (cargo in the SDK target, QML tests, emulator run)
├── src/
│   ├── main.rs               startup, type registration (QML module `Saillight 1.0`)
│   ├── sailfishapp.rs        rust side of the SailfishApp shim
│   ├── worker.rs             background threads, cancellation, bridge to the Qt thread
│   ├── config.rs             settings remembered between runs
│   ├── testing.rs            helpers shared by the rust unit tests
│   ├── scan/                 Qt-free
│   │   ├── tree.rs           size tree
│   │   ├── walk.rs           recursive walk filling it
│   │   ├── cache.rs          the tree saved in the app storage
│   │   ├── stale.rs          modification dates against the cache
│   │   └── owner.rs          user and group names, permissions as text
│   ├── chart/                Qt-free
│   │   ├── layout.rs         tree to sectors
│   │   ├── hit.rs            touch point to sector
│   │   └── labels.rs         names around the chart
│   ├── actions/
│   │   ├── delete.rs
│   │   └── compress.rs
│   └── model/
│       ├── state.rs          what the page shows and how it reacts, Qt-free
│       └── chart.rs          `ChartModel`
├── qml/                      installed to /usr/share/saillight/qml, not embedded
│   ├── saillight.qml         ApplicationWindow
│   ├── pages/MainPage.qml    the only page, pull-down menu
│   ├── components/           PathBar, Chart, ChartLabels, Details, ProgressRow
│   ├── js/logic.js           interface logic without visual items (formatting, path levels, colors)
│   ├── icons/                action icons missing from the theme, svg (created with the first one)
│   └── cover/                CoverPage
├── tests/qml/                QML unit tests (QtTest), rust unit tests stay next to the code
├── icons/<size>/saillight.png sizes 86x86, 108x108, 128x128, 172x172
├── saillight.desktop
├── rpm/saillight.spec
└── docs/
```

- build files, shim, scripts and rpm spec come from `../sailtree`, with the workarounds recorded in its journal

## build and test

- crate versions are limited to those building with the rust of the Sailfish SDK target (1.75, `rust-version` in `Cargo.toml`); `Cargo.lock` is regenerated on the host with `cargo generate-lockfile`
- `sfdk -c target=SailfishOS-5.1.0.11-i486 build-init` once, before the scripts can be used
- `scripts/target-cargo.sh <cargo args>` runs cargo inside the SDK target, eg. `scripts/target-cargo.sh test`. the host has no Qt
- `scripts/target-qmltest.sh` runs the QML unit tests of `tests/qml` inside the SDK target
- every step comes with its unit tests. filesystem tests work in temporary folders; `ChartModel` is tested without a GUI, its work running in place instead of on threads
- the build target has no screen: QML tests create components without a window, call the functions the touch areas call, and emit the `clicked` signal of buttons
- `sfdk -c target=SailfishOS-5.1.0.11-i486 build` produces `RPMS/saillight-*.rpm`
- `scripts/emulator-run.sh [--no-build] [file.png]` builds, installs in the SDK emulator, starts the app and saves a screenshot. it never targets a real device
- only the i486 target (emulator) is built for now
