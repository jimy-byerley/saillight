# implementation choices

## chart

- geometry is computed in rust, without Qt (`src/chart/`): sectors (two angles, two radii, color, state), the sector under a touch point, positions of the labels
- QML only draws: `Chart.qml` paints the sectors on a `Canvas` with its native arcs, and sends press, slide and release positions to the model
- the hovered sector is painted on a second canvas above the chart, so sliding does not repaint all the sectors
- the full chart is repainted only when its data changes (scan, change of folder, deletion, compression, depth)
- fallback if the canvas is too slow on a phone: a painted item written in the C++ embedded with `cpp!`, drawing the same sectors. only `Chart.qml` would change

## scan

- `src/scan/` knows nothing about the chart, `src/chart/` nothing about the filesystem. the size tree (`scan/tree.rs`) is what goes from one to the other, and what the cache stores

## background work

- no async runtime: scan and compression each run on a plain thread (`worker.rs`), with a cancellation flag checked between two steps of the work. results come back to the Qt thread with `qmetaobject::queued_callback`

## QML side

- a single object is exposed to QML, `ChartModel` (module `Saillight 1.0`): current folder, sectors, labels, hovered and selected sector, details, progress

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
│   │   └── stale.rs          modification dates against the cache
│   ├── chart/                Qt-free
│   │   ├── layout.rs         tree to sectors
│   │   ├── hit.rs            touch point to sector
│   │   └── labels.rs         names around the chart
│   ├── actions/
│   │   ├── delete.rs
│   │   └── compress.rs
│   └── model/
│       └── chart.rs          `ChartModel`
├── qml/                      installed to /usr/share/saillight/qml, not embedded
│   ├── saillight.qml         ApplicationWindow
│   ├── pages/MainPage.qml    the only page, pull-down menu
│   ├── components/           PathBar, Chart, ChartLabels, Details, ProgressRow
│   ├── js/logic.js           interface logic without visual items (formatting, path levels)
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
- `sfdk -c target=SailfishOS-5.1.0.11-i486 build` produces `RPMS/saillight-*.rpm`
- `scripts/emulator-run.sh [--no-build] [file.png]` builds, installs in the SDK emulator, starts the app and saves a screenshot. it never targets a real device
- only the i486 target (emulator) is built for now
