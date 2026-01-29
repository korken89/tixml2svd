use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::path::Path;

use xml::attribute::OwnedAttribute;
use xml::name::OwnedName;
use xml::reader::EventReader;
use xml::reader::XmlEvent::{EndElement, StartElement};
use xml::writer::EmitterConfig;

use crate::peripheral::process_peripheral_base;
use crate::writer::{
    write_comment, write_end, write_start, write_start_with_attr, write_tag, Args,
};

/// Used by process_device_base to open each peripheral file and
/// provide a xml parser for the file. It only makes sense to replace
/// this if you wish to run this code without file-based storage.
pub fn get_parser_from_filename(
    root: &str,
    filename: &str,
) -> io::Result<xml::EventReader<std::fs::File>> {
    let root_path = Path::new(root);
    let concat_path = root_path.with_file_name(filename);
    let fd_periph = File::open(&concat_path)?;
    Ok(EventReader::new(fd_periph))
}

/// Used by process_device_base to convert the TIXML <device> header
/// to the corresponding SVD <device> fields.
fn generate_device<O>(
    args: &Args,
    xml_out: &mut xml::EventWriter<&mut O>,
    device_attributes: &Vec<OwnedAttribute>,
    cpu_attributes: &Vec<OwnedAttribute>,
    endianness: &Option<String>,
) -> io::Result<()>
where
    O: io::Write,
{
    if args.no_device_info {
        return Ok(());
    }

    let mut f_id: Option<&str> = None;
    let mut f_hw_revision: Option<&str> = None;
    let mut f_description: Option<&str> = None;
    let mut f_isa: Option<String> = None;

    for attr in device_attributes {
        let xml::attribute::OwnedAttribute { name, value } = attr;
        let OwnedName {
            local_name: attr_name,
            ..
        } = name;
        match attr_name.as_ref() {
            "id" => {
                if !value.is_empty() {
                    f_id = Some(value)
                }
            }
            "description" => {
                if !value.is_empty() {
                    f_description = Some(value)
                }
            }
            _ => {}
        }
    }

    for attr in cpu_attributes {
        let xml::attribute::OwnedAttribute { name, value } = attr;
        let OwnedName {
            local_name: attr_name,
            ..
        } = name;
        match attr_name.as_ref() {
            "HW_revision" => {
                if !value.is_empty() {
                    f_hw_revision = Some(value)
                }
            }
            "isa" => {
                if !value.is_empty() {
                    f_isa = Some(if args.sanitize {
                        value.replace("Cortex_", "C")
                    } else {
                        value.to_string()
                    })
                }
            }
            _ => {}
        }
    }

    write_tag(args, xml_out, "name", f_id.unwrap_or("[unknown CPU]"))?;
    write_tag(args, xml_out, "version", f_hw_revision.unwrap_or("0.0"))?;
    write_tag(args, xml_out, "description", f_description.unwrap_or(""))?;
    write_start(args, xml_out, "cpu")?;
    write_tag(args, xml_out, "name", f_isa.as_deref().unwrap_or("other"))?;
    write_tag(args, xml_out, "revision", f_hw_revision.unwrap_or("0.0"))?;
    write_tag(
        args,
        xml_out,
        "endian",
        endianness.as_deref().unwrap_or("other"),
    )?;
    write_tag(args, xml_out, "mpuPresent", "true")?;
    write_tag(args, xml_out, "fpuPresent", "true")?;
    write_tag(args, xml_out, "nvicPrioBits", "3")?;
    write_tag(args, xml_out, "vendorSystickConfig", "false")?;
    write_end(args, xml_out)?;
    write_tag(args, xml_out, "addressUnitBits", "8")?;
    write_tag(args, xml_out, "width", "32")?;
    write_tag(args, xml_out, "size", "32")?;
    write_tag(args, xml_out, "access", "read-write")?;
    write_tag(args, xml_out, "resetValue", "0x00000000")?;
    write_tag(args, xml_out, "resetMask", "0xFFFFFFFF")
}

fn check_endianness(args: &Args, attributes: &Vec<OwnedAttribute>) -> Option<String> {
    let mut f_type: Option<&str> = None;
    let mut f_value: Option<&str> = None;
    let mut f_id: Option<&str> = None;

    for attr in attributes {
        let xml::attribute::OwnedAttribute { name, value } = attr;
        let value = if args.sanitize { value.trim() } else { value };
        let OwnedName {
            local_name: attr_name,
            ..
        } = name;
        match attr_name.as_ref() {
            "Type" => {
                if !value.is_empty() {
                    f_type = Some(value)
                }
            }
            "Value" => {
                if !value.is_empty() {
                    f_value = Some(value)
                }
            }
            "id" => {
                if !value.is_empty() {
                    f_id = Some(value)
                }
            }
            _ => {}
        }
    }

    f_type
        .filter(|t| *t == "stringfield")
        .and(f_id.filter(|t| *t == "Endianness"))
        .and(f_value)
        .map(|e| e.to_string())
}

/// Convert a TIXML device to SVD.
pub fn process_device<I, O>(args: &Args, fin: I, root_path: &str, fout: &mut O) -> io::Result<()>
where
    I: io::Read,
    O: io::Write,
{
    let mut xml_out = EmitterConfig::new()
        .perform_indent(true)
        .create_writer(fout);
    let parser = EventReader::new(fin);

    process_device_base(args, parser, &mut xml_out, &|x| {
        get_parser_from_filename(root_path, x)
    })
}

/// Convert a TIXML device to SVD.
pub fn process_device_base<I, O>(
    args: &Args,
    parser: xml::EventReader<I>,
    xml_out: &mut xml::EventWriter<&mut O>,
    fname2parser: &dyn Fn(&str) -> io::Result<xml::EventReader<std::fs::File>>,
) -> io::Result<()>
where
    I: io::Read,
    O: io::Write,
{
    let mut printed_peripherals_tag = true;
    let mut in_cpu_tag = false;
    let mut cpunum = 0;
    let mut endianness: Option<String> = None;
    let mut device_attributes: Vec<OwnedAttribute> = vec![];
    // Track which module hrefs have been processed and map to their first peripheral name
    let mut module_to_peripheral: HashMap<String, String> = HashMap::new();

    for e in parser {
        match e {
            Ok(StartElement {
                name,
                attributes,
                namespace: _namespace,
            }) => {
                if args.verbose > 0 {
                    eprintln!("Processing StartElement: {}", name);
                }
                let OwnedName {
                    local_name,
                    namespace: _,
                    prefix: _,
                } = name;
                match local_name.as_ref() {
                    "device" => {
                        write_start(args, xml_out, "device")?;
                        write_comment(
                            args,
                            xml_out,
                            "Created by tixml2svd; https://github.com/dhoove/tixml2svd",
                        )?;

                        device_attributes = attributes;
                    }
                    "cpu" => {
                        in_cpu_tag = true;
                        if cpunum != args.cpunum {
                            continue;
                        }
                        generate_device(
                            args,
                            xml_out,
                            &device_attributes,
                            &attributes,
                            &endianness,
                        )?;
                        printed_peripherals_tag = false;
                    }
                    "property" => {
                        if !in_cpu_tag {
                            continue;
                        }

                        endianness = endianness.or_else(|| check_endianness(args, &attributes));
                    }
                    "instance" => {
                        if !in_cpu_tag | (cpunum != args.cpunum) {
                            if args.verbose > 0 {
                                eprintln!(
                                    "Skipping cpu instance; in_cpu_tag='{}', cpunum='{}'",
                                    in_cpu_tag, cpunum
                                );
                            }
                            continue;
                        }

                        let mut f_baseaddr: Option<String> = None;
                        let mut _f_endaddr: Option<String> = None;
                        let mut f_size: Option<String> = None;
                        let mut f_id: Option<String> = None;
                        let mut f_href: Option<String> = None;

                        for attr in attributes {
                            let xml::attribute::OwnedAttribute { name, value } = attr;
                            let value = if args.sanitize {
                                String::from(value.trim())
                            } else {
                                value
                            };
                            let OwnedName {
                                local_name: attr_name,
                                ..
                            } = name;
                            match attr_name.as_ref() {
                                "baseaddr" => {
                                    if !value.is_empty() {
                                        f_baseaddr = Some(value)
                                    }
                                }
                                "endaddr" => {
                                    if !value.is_empty() {
                                        _f_endaddr = Some(value)
                                    }
                                }
                                "size" => {
                                    if !value.is_empty() {
                                        f_size = Some(value)
                                    }
                                }
                                "id" => {
                                    if !value.is_empty() {
                                        f_id = Some(if args.sanitize {
                                            value.replace("-", "_")
                                        } else {
                                            value
                                        })
                                    }
                                }
                                "href" => {
                                    if !value.is_empty() {
                                        f_href = Some(value)
                                    }
                                }
                                unknown => {
                                    if args.verbose > 0 {
                                        eprintln!(
                                            "Ignoring unknown key '{}' for '{}'",
                                            unknown, local_name
                                        );
                                    };
                                }
                            };
                        }

                        let skip = match f_href {
                            Some(ref href) => !href.clone().starts_with("../Modules/"),
                            None => true,
                        };

                        if let Some(id) = f_id {
                            // If no ID present, ignore the module (TI-internal?)
                            if skip {
                                eprintln!("Sub-instance href does not start with Modules, or is missing. Skipping: '{:?}'", id);
                            } else if id == "Cp15" || id == "Vfp" {
                                eprintln!(
                                    "Peripheral id {:?} suggests co-processor registers; Ignoring",
                                    id
                                );
                            } else if !id.is_empty() {
                                if !printed_peripherals_tag {
                                    write_start(args, xml_out, "peripherals")?;
                                    printed_peripherals_tag = true;
                                }

                                // Check if this module href has been seen before
                                if let Some(ref href) = f_href {
                                    if let Some(first_peripheral) = module_to_peripheral.get(href) {
                                        // Use derivedFrom - write minimal peripheral
                                        write_start_with_attr(
                                            args,
                                            xml_out,
                                            "peripheral",
                                            &[("derivedFrom", first_peripheral)],
                                        )?;
                                        write_tag(args, xml_out, "name", &id)?;
                                        if let Some(ref baseaddr) = f_baseaddr {
                                            write_tag(args, xml_out, "baseAddress", baseaddr)?;
                                        }
                                        write_end(args, xml_out)?;
                                        continue;
                                    } else {
                                        // First time seeing this module - record it
                                        module_to_peripheral.insert(href.clone(), id.clone());
                                    }
                                }

                                // Write full peripheral definition
                                write_start(args, xml_out, "peripheral")?;
                                write_tag(args, xml_out, "name", &id)?;

                                if let Some(baseaddr) = f_baseaddr {
                                    write_tag(args, xml_out, "baseAddress", &baseaddr)?;
                                }

                                match f_size {
                                    Some(size) => {
                                        write_start(args, xml_out, "addressBlock")?;
                                        write_tag(args, xml_out, "offset", "0")?;
                                        write_tag(args, xml_out, "size", &size)?;
                                        write_tag(args, xml_out, "usage", "registers")?;
                                        write_end(args, xml_out)?;
                                    }
                                    None => {
                                        if !args.silent {
                                            eprintln!("Peripheral has no size for {}", local_name);
                                        }
                                    }
                                }

                                if let Some(href) = f_href {
                                    if !args.silent {
                                        eprintln!("Processing peripheral file: {:?}", &href);
                                    }
                                    let parser = fname2parser(&href)?;
                                    process_peripheral_base(args, parser, xml_out)?;
                                }

                                write_end(args, xml_out)?;
                            }
                        }
                    }
                    unknown => {
                        if args.verbose > 0 {
                            eprintln!("Ignoring unknown start element key '{}'", unknown);
                        }
                    }
                }
            }

            Ok(EndElement { name }) => {
                if args.verbose > 0 {
                    eprintln!("Processing EndElement: {}", name);
                }
                let OwnedName { local_name, .. } = name;
                match local_name.as_ref() {
                    "device" => {
                        write_end(args, xml_out)?;
                    }
                    "cpu" => {
                        if cpunum == args.cpunum {
                            if printed_peripherals_tag {
                                write_end(args, xml_out)?;
                            }

                            printed_peripherals_tag = true;
                        }

                        in_cpu_tag = false;
                        cpunum += 1;
                    }
                    "instance" => {}
                    unknown => {
                        if args.verbose > 0 {
                            eprintln!("Ignoring unknown end element key '{}'", unknown);
                        }
                    }
                }
            }

            Err(e) => {
                return Err(io::Error::other(e.to_string()));
            }
            _ => {}
        }
    }
    Ok(())
}
