/// This utility creates
/// [SVD](https://www.keil.com/pack/doc/CMSIS/SVD/html/svd_Format_pg.html)
/// files from the Texas-Instruments XML (called TIXML from now on) device
/// and peripheral descriptor files.
extern crate xml;

mod device;
mod peripheral;
mod writer;

pub use device::{get_parser_from_filename, process_device, process_device_base};
pub use peripheral::{process_peripheral, process_peripheral_base};
pub use writer::Args;
