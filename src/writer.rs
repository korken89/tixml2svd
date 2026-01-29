use std::io;
use xml::writer;

/// This structure contains arguments used to customize the behavior of tixml2svd.
pub struct Args {
    /// Produce no output other than the SVD data
    pub(crate) silent: bool,
    /// Produce additional output, given 0, 1, 2, etc.
    pub(crate) verbose: u32,
    // Expect a peripheral file instead of a device file.
    pub(crate) peripheral_only: bool,
    // Sanitize the SVD file (for svd2rust, for example)
    pub(crate) sanitize: bool,
    // Do not generate fake device info in file header
    pub(crate) no_device_info: bool,
    // If there are several CPUs, read peripherals from CPU 0, 1, or 2, for example.
    pub(crate) cpunum: u32,
}

impl Args {
    pub fn new(
        silent: bool,
        verbose: u32,
        peripheral_only: bool,
        sanitize: bool,
        no_device_info: bool,
        cpunum: u32,
    ) -> Args {
        Args {
            silent,
            verbose,
            peripheral_only,
            sanitize,
            no_device_info,
            cpunum,
        }
    }
}

pub(crate) fn write_access<O>(
    args: &Args,
    xml_out: &mut xml::EventWriter<&mut O>,
    ti_access: &str,
) -> io::Result<()>
where
    O: io::Write,
{
    let access = match ti_access {
        "RO" => "read-only",
        "WO" => "write-only",
        "RW" => "read-write",
        "R=1/W=0" => "read-only",
        "R=0/W=1" => "write-only",
        "R=1/W=1" => "read-write",
        "R" => "read-only",
        "W" => "write-only",
        "R/W" => "read-write",
        "R/W1TC" => "read-write",
        "R/W1TS" => "read-write",
        "W1TC" => "write-only",
        "W1TS" => "write-only",
        "R/W1C" => "read-write",
        "R/WD" => "read-write",
        "R/WI" => "read-write",
        "R/W0TC" => "read-write",
        "R/WTC" => "read-write",
        "R/WTD" => "read-write",
        "R/R/WONCE" => "read-writeOnce",
        "NU1" | "NU2" | "N/A" => return Ok(()), // Not used - skip silently
        unknown => {
            if !args.silent {
                eprintln!("Ignoring unknown access key '{}'", unknown);
            }
            return Ok(());
        }
    };

    write_tag(args, xml_out, "access", access)
}

pub(crate) fn write_start<O>(
    args: &Args,
    xml_out: &mut xml::EventWriter<&mut O>,
    element: &str,
) -> io::Result<()>
where
    O: io::Write,
{
    let event: writer::XmlEvent = writer::XmlEvent::start_element(element).into();
    if args.verbose > 2 {
        eprintln!("Writing start-tag: {:?}", event);
    }
    match xml_out.write(event) {
        Ok(x) => Ok(x),
        Err(x) => Err(io::Error::other(x.to_string())),
    }
}

pub(crate) fn write_comment<O>(
    args: &Args,
    xml_out: &mut xml::EventWriter<&mut O>,
    data: &str,
) -> io::Result<()>
where
    O: io::Write,
{
    let event: writer::XmlEvent = writer::XmlEvent::comment(data);
    if args.verbose > 2 {
        eprintln!("Writing comment: {:?}", event);
    }
    match xml_out.write(event) {
        Ok(x) => Ok(x),
        Err(x) => Err(io::Error::other(x.to_string())),
    }
}

pub(crate) fn write_content<O>(
    args: &Args,
    xml_out: &mut xml::EventWriter<&mut O>,
    content: &str,
) -> io::Result<()>
where
    O: io::Write,
{
    let event: writer::XmlEvent = writer::XmlEvent::characters(content);
    if args.verbose > 2 {
        eprintln!("Writing content: {:?}", event);
    }
    match xml_out.write(event) {
        Ok(x) => Ok(x),
        Err(x) => Err(io::Error::other(x.to_string())),
    }
}

pub(crate) fn write_end<O>(args: &Args, xml_out: &mut xml::EventWriter<&mut O>) -> io::Result<()>
where
    O: io::Write,
{
    let event: writer::XmlEvent = writer::XmlEvent::end_element().into();
    if args.verbose > 2 {
        eprintln!("Writing end-tag: {:?}", event);
    }
    match xml_out.write(event) {
        Ok(x) => Ok(x),
        Err(x) => Err(io::Error::other(x.to_string())),
    }
}

pub(crate) fn write_tag<O>(
    args: &Args,
    xml_out: &mut xml::EventWriter<&mut O>,
    element: &str,
    content: &str,
) -> io::Result<()>
where
    O: io::Write,
{
    write_start(args, xml_out, element)?;
    write_content(args, xml_out, content)?;
    write_end(args, xml_out)?;
    Ok(())
}

pub(crate) fn write_start_with_attr<O>(
    args: &Args,
    xml_out: &mut xml::EventWriter<&mut O>,
    name: &str,
    attrs: &[(&str, &str)],
) -> io::Result<()>
where
    O: io::Write,
{
    let mut element = writer::XmlEvent::start_element(name);
    for (key, value) in attrs {
        element = element.attr(*key, value);
    }
    if args.verbose > 2 {
        eprintln!("Writing start-tag with attrs: {:?}", name);
    }
    xml_out
        .write(element)
        .map_err(|e| io::Error::other(e.to_string()))
}
