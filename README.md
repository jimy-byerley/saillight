# Saillight

A storage size visualizer for Sailfish OS, in the spirit of Filelight, written in Rust with a QML (Silica) interface.

The sizes of the current folder are shown as a ring chart: each ring is a level of folders, each sector an item, proportional to the space it uses on disk.

- browse by the chart: tap a sector to select it, slide to change the target, tap the centre to go to the parent folder, tap a level of the path to jump to it
- names of the items around the chart, with arrows to their sectors; they follow the selected folder
- details of the selection (path, size, date, permissions, owner), and how full each storage is when nothing is selected
- free space: delete, or compress as zip, gz or 7z (folders go through tar, except for zip), the original being removed once the archive is verified
- unreadable folders and other storages are not counted, and marked in the chart by a thick line listing them
- sizes are cached between runs, with a warning when the folder changed since
- cover with the current folder, its share of the storage it is in, and a refresh action

Design documents are in `docs/`:

| File | Content |
|---|---|
| `docs/specifications.md` | what the app must do |
| `docs/implementation.md` | design choices and where things are in the code |
| `docs/journal.md` | dated issues, workarounds and decision changes |

## Quickstart

### Requirements

- the [Sailfish SDK](https://docs.sailfishos.org/Tools/Sailfish_SDK/) (developed with 3.13.5), with a build target for Sailfish OS 5.1
- `sfdk` at `~/jolla/sdk/bin/sfdk` (what the scripts assume; override with the `SFDK` or `SDK` variables)

Rust is not needed on the development machine: the build uses the Rust toolchain of the SDK target (1.75).

### Build

From the project folder, the first time:

```sh
sfdk -c target=SailfishOS-5.1.0.11-i486 build-init
```

then:

```sh
sfdk -c target=SailfishOS-5.1.0.11-i486 build
```

The package lands in `RPMS/saillight-<version>.i486.rpm`. A release build from scratch takes a few minutes.

For a phone, use its target instead, eg. `SailfishOS-5.1.0.11-aarch64` for a Jolla C2. The `aarch64` package builds, but only the `i486` one (the SDK emulator) has been run and tested so far.

### Install

In the SDK emulator, build, install, start the app and save a screenshot in one command:

```sh
scripts/emulator-run.sh                      # screenshot in target/screenshot.png
scripts/emulator-run.sh --no-build shot.png  # reuse the last package
```

On a device, copy the rpm matching its architecture and install it as root:

```sh
devel-su rpm -U --force saillight-*.rpm
```

The app is launched without the Sailjail sandbox (`Sandboxing=Disabled`), so it can look at the whole filesystem. What the user cannot read is shown as not counted. It has not been tried on a real phone yet: it deletes and compresses files, so try it on unimportant data first.

## Development

### Setup

1. Install the Sailfish SDK with its build engine, the `i486` target and the emulator.
2. Run `build-init` as shown in the quickstart.
3. Start the emulator from the SDK (`sfdk emulator start` or the Qt Creator of the SDK).

Things to know about the SDK, detailed in `docs/journal.md`:

- The build engine's disk is small by default (8 GB), which a Rust build fills. It can be grown with `sfdk engine stop; sfdk engine set vm.storage-size=20480; sfdk engine start`. Builds put their output on that disk, in `/home/mersdk/cargo-target/saillight`, because the shared project folder cannot hold it.
- Cargo runs one job at a time with a few `SB2_RUST_*` settings: without them it can freeze under the SDK's sandbox.
- Crate versions are limited to those building with Rust 1.75. After changing `Cargo.toml`, update `Cargo.lock` on the development machine with `cargo generate-lockfile`.

### Code layout

```
src/            Rust: startup, worker threads, settings
  scan/         size tree of a folder, its cache, change detection, storages (no Qt)
  chart/        sectors, touch point to sector, placement of the names (no Qt)
  actions/      delete and compress
  model/        the object shown to QML, and its Qt-free logic
qml/            interface: page, components, cover, icons, logic.js
tests/qml/      QML unit tests
rpm/            package spec
scripts/        development helpers
```

Geometry is computed in Rust and only drawn by QML. Logic is kept out of the Qt types where possible, so it can be unit tested without a screen. `docs/implementation.md` has the guideline for where new files go.

### Launcher icon

`python3 scripts/make-icons.py` (needs Pillow) writes `icons/saillight.svg`, the reference drawing, and the png files of each size from the same description: a ring chart in white on a grey circle, one of the base shapes of the [Sailfish app icon guide](https://sailfishos.org/design/icons/). Change the description in the script, not the files.

### Tests

Unit tests run inside the SDK target, as the development machine has no Qt or Silica:

```sh
scripts/target-cargo.sh test   # Rust unit tests
scripts/target-qmltest.sh      # QML unit tests (QtTest)
```

`scripts/target-cargo.sh` passes its arguments to cargo in the target, so `scripts/target-cargo.sh build` or `clippy` work the same way.

Tests that touch files only work in temporary folders. The build target has no screen, so QML tests create components without a window and call what the touch areas call.

### Trying it in the emulator

`scripts/emulator-run.sh` only ever talks to the emulator (ssh on `127.0.0.1:2223`), never to a real device. The app's log is in `~/saillight.log` of the emulator's user.

Touches cannot be simulated in the emulator from outside. `scripts/emulator-drive.sh` runs javascript statements one by one in the main page of the installed app, with a screenshot after each:

```sh
scripts/emulator-drive.sh target/drive \
    'chartModel.setFolder("/usr")' \
    'chartModel.select(0)' \
    'chartModel.enterSelected()'
```

It copies the project's QML to the emulator first, so QML changes need no rebuild.

## Status

Working in the emulator: scan with progress, chart, names, selection, details, storage gauges, navigation, cache and change warning, deletion, compression in the three formats, cover.

Not done yet: a try on a real phone (touch handling and drawing speed are only known from the emulator), packaging for the Chum repository, and the choice of a license (the rpm spec has a placeholder).
