//! Human-readable CLI output; the library and JSON retain complete paths.
use comfy_table::{ContentArrangement, Table, presets::ASCII_FULL_CONDENSED};
use humansize::{BINARY, format_size};
use jartender::{EulaState, Server};
use std::io::{self, Write};

fn launch_entry(server: &Server) -> String {
    if let Some(selected) = &server.selected_jar {
        return format!("{} (selected)", selected.to_string_lossy());
    }
    match server.jars.as_slice() {
        [jar] => jar.to_string_lossy().into_owned(),
        [] => match server.argument_files.as_slice() {
            [file] => {
                let filename = file.file_name().unwrap_or_default().to_string_lossy();
                let version = file
                    .parent()
                    .and_then(|p| p.file_name())
                    .unwrap_or_default()
                    .to_string_lossy();
                format!("@.../{version}/{filename}")
            }
            files => format!("{} files (choose --args-file)", files.len()),
        },
        jars => format!("{} JARs (choose --jar)", jars.len()),
    }
}

fn eula_cell(state: EulaState) -> &'static str {
    match state {
        EulaState::Accepted => "yes",
        EulaState::NotAccepted => "no",
        EulaState::NotRequired => "n/a",
    }
}

pub fn write(out: &mut impl Write, servers: &[Server]) -> io::Result<()> {
    if servers.is_empty() {
        return writeln!(out, "No servers found.");
    }
    let mut table = Table::new();
    table
        .load_style(ASCII_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(["SERVER", "TYPE", "VERSION", "MAP SIZE", "EULA", "LAUNCH"]);
    for server in servers {
        table.add_row([
            server.name.escape_debug().to_string(),
            server.server_type.to_owned(),
            server
                .version
                .as_deref()
                .unwrap_or("-")
                .escape_debug()
                .to_string(),
            server
                .map_size_bytes
                .map(|bytes| format_size(bytes, BINARY))
                .unwrap_or_else(|| "-".to_owned()),
            eula_cell(server.eula).to_owned(),
            launch_entry(server).escape_debug().to_string(),
        ]);
    }
    writeln!(out, "{table}")?;
    writeln!(
        out,
        "{} {}",
        servers.len(),
        if servers.len() == 1 {
            "server"
        } else {
            "servers"
        }
    )
}
