use std::collections::HashSet;
use std::io;
use std::str::FromStr;

use xml::name::OwnedName;
use xml::reader::EventReader;
use xml::reader::XmlEvent::{EndElement, StartElement};
use xml::writer::EmitterConfig;

use crate::writer::{write_access, write_end, write_start, write_tag, Args};

/// Check if a reset value string has an errant `0x` prefix (a common TI XML bug
/// where e.g. `0x32767` was meant as decimal `32767`). Returns the corrected
/// decimal value if the decimal interpretation fits in the field.
fn fix_errant_hex_prefix(val_str: &str, reg_width: u32, end_int: u32) -> Option<u64> {
    if !val_str.starts_with("0x") && !val_str.starts_with("0X") {
        return None;
    }
    let hex_str = &val_str[2..];
    if !hex_str.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let dec_val = u64::from_str(hex_str).ok()?;
    let dec_overflow = dec_val >> (reg_width - end_int);
    if dec_overflow != 0 {
        return None;
    }
    Some(dec_val)
}

fn get_name_from_description(description: &str) -> String {
    let replaces = [
        (',', '_'),
        ('.', '_'),
        (':', '\0'),
        ('/', '_'),
        ('#', '\0'),
        ('-', '_'),
        (' ', '_'),
    ];
    let mut name = description
        .replace('-', "")
        .split_whitespace()
        .take(3)
        .collect::<Vec<&str>>()
        .join("_");
    name = name.split("#br#").next().unwrap_or("").to_string();
    name = name.split(':').next().unwrap_or("").to_string();
    for (from, to) in &replaces {
        name = name.replace(&from.to_string(), &to.to_string());
    }
    name.to_uppercase()
}

/// Convert a TIXML peripheral to SVD.
pub fn process_peripheral<I, O>(args: &Args, fin: I, fout: &mut O) -> io::Result<()>
where
    I: io::Read,
    O: io::Write,
{
    let mut xml_out = EmitterConfig::new()
        .perform_indent(true)
        .create_writer(fout);
    let parser = EventReader::new(fin);

    process_peripheral_base(args, parser, &mut xml_out)
}

/// Convert a TIXML peripheral to SVD.
pub fn process_peripheral_base<I, O>(
    args: &Args,
    parser: xml::EventReader<I>,
    xml_out: &mut xml::EventWriter<&mut O>,
) -> io::Result<()>
where
    I: io::Read,
    O: io::Write,
{
    let mut printed_registers_tag = false;

    let mut printed_fields_tag = false;

    #[allow(non_snake_case)]
    let mut printed_enumeratedValues_tag = false;

    // Temporary storage to check for resetval overflow
    let mut register_width = None;

    let mut register_reset_value = None;

    let mut f_used_registers = None;

    let mut f_used_enumerations = None;
    let mut f_parent_reg_name = None;

    for e in parser {
        match e {
            Ok(StartElement {
                name,
                attributes,
                namespace: _,
            }) => {
                if args.verbose > 0 {
                    eprintln!("Processing StartElement: {}", name);
                }
                let OwnedName { local_name, .. } = name;
                match local_name.as_ref() {
                    "module" => {
                        if args.sanitize {
                            f_used_registers = Some(HashSet::new());
                        }

                        if args.peripheral_only {
                            write_start(args, xml_out, "peripheral")?;
                        }
                        printed_registers_tag = false;
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
                                "HW_revision" => (),
                                "XML_version" => (),
                                "noNamespaceSchemaLocation" => (),
                                "id" => {
                                    if args.peripheral_only {
                                        write_tag(args, xml_out, "name", &value)?;
                                    }
                                }
                                "value" => {
                                    if args.peripheral_only {
                                        write_tag(args, xml_out, "value", &value)?;
                                    }
                                }
                                "token" => (),
                                "description" => {
                                    write_tag(args, xml_out, "description", &value)?;
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
                    }

                    "register" => {
                        let mut f_id: Option<String> = None;
                        let mut f_value: Option<String> = None;
                        let mut f_width: Option<String> = None;
                        let mut f_description: Option<String> = None;
                        // Assume access is read-write if not specified, and let further restrictions be applied by the bitfilelds
                        let mut f_rwaccess: Option<String> = Some("RW".to_string());
                        let mut f_offset: Option<String> = None;
                        let mut f_resetval: Option<String> = None;

                        if !printed_registers_tag {
                            printed_registers_tag = true;
                            write_start(args, xml_out, "registers")?;
                        }

                        write_start(args, xml_out, "register")?;
                        printed_fields_tag = false;
                        register_reset_value = None;

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
                                "id" => {
                                    if !value.is_empty() {
                                        f_id = Some(value)
                                    }
                                }
                                "value" => {
                                    if !value.is_empty() {
                                        f_value = Some(value)
                                    }
                                }
                                "width" => {
                                    if !value.is_empty() {
                                        f_width = Some(value)
                                    }
                                }
                                "acronym" => (),
                                "description" => {
                                    if !value.is_empty() {
                                        f_description = Some(value)
                                    }
                                }
                                "rwaccess" => {
                                    if !value.is_empty() {
                                        f_rwaccess = Some(value)
                                    }
                                }
                                "offset" => {
                                    if !value.is_empty() {
                                        f_offset = Some(value)
                                    }
                                }
                                "resetval" => {
                                    if !value.is_empty() {
                                        f_resetval = Some(value)
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

                        if let Some(id) = f_id.clone() {
                            let unique_name = match f_used_registers {
                                Some(ref mut used_registers) => {
                                    let mut regname = id;
                                    while !used_registers.insert(regname.clone()) {
                                        eprintln!(
                                            "Non-unique register name {}. Appending underline.",
                                            regname
                                        );
                                        regname.push('_');
                                    }
                                    regname
                                }
                                None => id,
                            };
                            f_parent_reg_name = Some(unique_name.clone());
                            write_tag(args, xml_out, "name", &unique_name)?;
                        }
                        if let Some(value) = f_value {
                            write_tag(args, xml_out, "value", &value)?;
                        }
                        if let Some(offset) = f_offset {
                            write_tag(args, xml_out, "addressOffset", &offset)?;
                        }
                        if let Some(width) = f_width {
                            let w: u32 = width.parse().unwrap();
                            register_width = Some(w);
                            write_tag(args, xml_out, "size", &width)?;
                        }
                        if let Some(description) = f_description {
                            write_tag(args, xml_out, "description", &description)?;
                        } else if let Some(id) = f_id {
                            write_tag(args, xml_out, "description", &id)?;
                        } else {
                            write_tag(args, xml_out, "description", "--")?;
                        }
                        if let Some(rwaccess) = f_rwaccess {
                            write_access(args, xml_out, &rwaccess)?;
                        }
                        if let Some(resetval) = f_resetval {
                            let resetval: u64 = resetval.parse().unwrap();
                            register_reset_value = Some(resetval);
                        }
                    }

                    "bitfield" => {
                        if !printed_fields_tag {
                            printed_fields_tag = true;
                            write_start(args, xml_out, "fields")?;
                        }

                        write_start(args, xml_out, "field")?;
                        printed_enumeratedValues_tag = false;

                        let mut f_name: Option<String> = None;
                        let mut f_range: Option<String> = None;
                        let mut f_begin: Option<u32> = None;
                        let mut f_width: Option<u32> = None;
                        let mut f_end: Option<u32> = None;
                        let mut f_rwaccess: Option<String> = None;
                        let mut f_description: Option<String> = None;
                        let mut f_reset_value: Option<u64> = None;
                        let mut f_resetval_str: Option<String> = None;

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
                                "id" => {
                                    if !value.is_empty() {
                                        f_name = Some(value)
                                    }
                                }
                                "range" => {
                                    if !value.is_empty() {
                                        f_range = Some(value)
                                    }
                                }
                                "begin" => {
                                    if !value.is_empty() {
                                        f_begin = Some(u32::from_str(&value).unwrap())
                                    }
                                }
                                "width" => {
                                    if !value.is_empty() {
                                        f_width = Some(u32::from_str(&value).unwrap())
                                    }
                                }
                                "end" => {
                                    if !value.is_empty() {
                                        f_end = Some(u32::from_str(&value).unwrap())
                                    }
                                }
                                "rwaccess" => {
                                    if !value.is_empty() {
                                        f_rwaccess = Some(value)
                                    }
                                }
                                "description" => {
                                    if !value.is_empty() {
                                        f_description = Some(value)
                                    }
                                }
                                "resetval" => {
                                    f_resetval_str = Some(value.clone());
                                    f_reset_value =
                                        if value.starts_with("0x") || value.starts_with("0X") {
                                            u64::from_str_radix(&value[2..], 16).ok()
                                        } else {
                                            u64::from_str(&value).ok()
                                        };
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

                        if let Some(end_int) = f_end {
                            // Trust f_begin more than f_width
                            if let Some(begin_int) = f_begin {
                                f_width = Some(begin_int - end_int + 1)
                            }

                            if let Some(mut reset_value) = f_reset_value {
                                let reg_width: u32 = register_width.unwrap_or(32);

                                if let Some(width_int) = f_width {
                                    if end_int + width_int > reg_width {
                                        return Err(io::Error::other(format!("Field {:?} with offset {} and width {} too big for register of width {}.", f_name, end_int, width_int, reg_width)));
                                    }
                                }

                                if end_int < reg_width {
                                    let mut overflow = reset_value >> (reg_width - end_int);

                                    if overflow != 0 {
                                        if let Some(ref val_str) = f_resetval_str {
                                            if let Some(dec_val) =
                                                fix_errant_hex_prefix(val_str, reg_width, end_int)
                                            {
                                                if !args.silent {
                                                    eprintln!(
                                                        "Resetval '{}' (hex {}) overflows field, using decimal {} instead",
                                                        val_str, reset_value, dec_val
                                                    );
                                                }
                                                reset_value = dec_val;
                                                overflow = 0;
                                            }
                                        }
                                    }

                                    if overflow == 0 {
                                        let shifted_reset_value = reset_value << end_int;
                                        if let Some(rrv) = register_reset_value {
                                            register_reset_value = Some(rrv | shifted_reset_value)
                                        } else {
                                            register_reset_value = Some(shifted_reset_value);
                                        }
                                    } else if args.sanitize {
                                        eprintln!(
                                            "Resetval {} too big for field {:?}.",
                                            reset_value, f_name
                                        );
                                    } else {
                                        return Err(io::Error::other(format!(
                                            "Resetval {} too big for field {:?}.",
                                            reset_value, f_name
                                        )));
                                    }
                                }
                            }
                        }

                        if f_name.is_none() && args.sanitize {
                            if let Some(description) = f_description.clone() {
                                f_name = Some(get_name_from_description(&description));
                            } else {
                                let parent_reg_name = f_parent_reg_name.clone();
                                let bit_width = f_width;
                                let bit_offset = f_end;

                                // Create the formatted string
                                let name_value = format!(
                                    "{}_W{}_O{}",
                                    parent_reg_name.unwrap_or_default(),
                                    bit_width.unwrap_or_default(),
                                    bit_offset.unwrap_or_default()
                                );
                                // Assign the formatted string to f_name
                                f_name = Some(name_value);
                            }
                        }

                        if let Some(name) = f_name {
                            write_tag(args, xml_out, "name", &name)?;
                        }
                        if let Some(description) = f_description {
                            if let (Some(begin), Some(end)) = (f_begin, f_end) {
                                let desc = format!("[{}:{}] {}", begin, end, description);
                                write_tag(args, xml_out, "description", &desc)?;
                            } else {
                                write_tag(
                                    args,
                                    xml_out,
                                    "description",
                                    if description.is_empty() {
                                        "--"
                                    } else {
                                        &description
                                    },
                                )?;
                            }
                        }

                        if let Some(width) = f_width {
                            write_tag(args, xml_out, "bitWidth", &width.to_string())?;
                        }
                        if let Some(end) = f_end {
                            write_tag(args, xml_out, "bitOffset", &end.to_string())?;
                        }

                        // bitRange unlikely to work with svd2rust
                        if !args.sanitize {
                            if let Some(range) = f_range {
                                write_tag(args, xml_out, "bitRange", &range)?;
                            }
                        }
                        if let Some(rwaccess) = f_rwaccess {
                            write_access(args, xml_out, &rwaccess)?;
                        }
                    }

                    "bitenum" => {
                        if !printed_enumeratedValues_tag {
                            printed_enumeratedValues_tag = true;
                            write_start(args, xml_out, "enumeratedValues")?;
                            if args.sanitize {
                                f_used_enumerations = Some(HashSet::new());
                            }
                        }

                        let mut f_id: Option<String> = None;
                        let mut f_value: Option<String> = None;
                        let mut f_description: Option<String> = None;

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
                                "id" => {
                                    if !value.is_empty() {
                                        f_id = Some(value)
                                    }
                                }
                                "value" => {
                                    if !value.is_empty() {
                                        f_value = Some(value)
                                    }
                                }
                                "description" => {
                                    if !value.is_empty() {
                                        f_description = Some(value)
                                    }
                                }
                                "token" => (),
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

                        if let Some(value) = f_value {
                            let do_it: bool = match f_used_enumerations {
                                Some(ref mut used_enumerations) => {
                                    used_enumerations.insert(value.clone())
                                }
                                None => true,
                            };
                            if do_it {
                                write_start(args, xml_out, "enumeratedValue")?;
                                if let Some(id) = f_id {
                                    write_tag(args, xml_out, "name", &id)?;
                                } else if args.sanitize {
                                    // If id is missing, use value instead
                                    write_tag(args, xml_out, "name", &value)?;
                                }
                                write_tag(args, xml_out, "value", &value)?;
                                if let Some(description) = f_description {
                                    write_tag(
                                        args,
                                        xml_out,
                                        "description",
                                        if description.is_empty() {
                                            "--"
                                        } else {
                                            &description
                                        },
                                    )?;
                                }
                                write_end(args, xml_out)?;
                            } else {
                                eprintln!("Non-unique enumeration name {}. Ignoring.", value);
                            }
                        }
                    }
                    unknown => {
                        if args.verbose > 0 {
                            eprintln!("Ignoring unknown start element key '{}'", unknown);
                        }
                    }
                };
            }
            Ok(EndElement { name }) => {
                if args.verbose > 0 {
                    eprintln!("Processing EndElement: {}", name);
                }
                let OwnedName {
                    local_name,
                    prefix: _,
                    namespace: _,
                } = name;
                match local_name.as_ref() {
                    "module" => {
                        f_used_registers = None;

                        if printed_registers_tag {
                            printed_registers_tag = false;
                            write_end(args, xml_out)?;
                        }
                        if args.peripheral_only {
                            write_end(args, xml_out)?;
                        }
                    }

                    "register" => {
                        if printed_fields_tag {
                            printed_fields_tag = false;
                            write_end(args, xml_out)?;
                        }

                        if let Some(value) = register_reset_value {
                            let hex_reset = format!("0x{:X}", value);
                            write_tag(args, xml_out, "resetValue", &hex_reset)?;
                        } else {
                            // For svd2rust
                            let rv = "0";
                            write_tag(args, xml_out, "resetValue", rv)?;
                        }

                        register_width = None;
                        write_end(args, xml_out)?;
                    }

                    "bitfield" => {
                        if printed_enumeratedValues_tag {
                            printed_enumeratedValues_tag = false;
                            write_end(args, xml_out)?;
                            f_used_enumerations = None;
                        }
                        write_end(args, xml_out)?;
                    }

                    "bitenum" => {}
                    unknown => {
                        if args.verbose > 0 {
                            eprintln!("Ignoring unknown end element key '{}'", unknown);
                        }
                    }
                };
            }
            Err(e) => {
                return Err(io::Error::other(e.to_string()));
            }
            _ => {}
        }
    }
    Ok(())
}
