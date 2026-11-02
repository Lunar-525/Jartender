//! A host application chooses configuration and owns all process/console policy.
use jartender::{LaunchOptions, discover, prepare_launch};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(
        args.next()
            .ok_or("usage: launcher ABSOLUTE_ROOT [SERVER JAVA]")?,
    );
    for server in discover(&root)? {
        println!("{} ({})", server.name, server.server_type);
    }
    if let Some(server) = args.next() {
        let server = server
            .into_string()
            .map_err(|_| "server name must be UTF-8")?;
        let java = args.next().ok_or("supply a Java executable explicitly")?;
        let plan = prepare_launch(&root, &LaunchOptions::new(server, java))?;
        plan.check_eula()?;
        // The host can set piped stdio, retain a Child, or use its own process manager.
        let status = plan.to_command().status()?;
        println!("server exited: {status}");
    }
    Ok(())
}
