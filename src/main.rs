use std::ffi::CStr;

mod actions;
mod chart;
mod config;
mod model;
mod sailfishapp;
mod scan;
#[cfg(test)]
mod testing;
mod worker;

fn main() {
    let code = sailfishapp::run(|| {
        // usable from QML with `import Saillight 1.0`
        let module = CStr::from_bytes_with_nul(b"Saillight\0").expect("valid C string");
        let name = CStr::from_bytes_with_nul(b"ChartModel\0").expect("valid C string");
        qmetaobject::qml_register_type::<model::chart::ChartModel>(module, 1, 0, name);
    });
    std::process::exit(code);
}
