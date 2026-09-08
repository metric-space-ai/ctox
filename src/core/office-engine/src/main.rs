use ctox_office_engine as office_engine;

#[path = "../../business_os/office_cli.rs"]
mod office_cli;

fn main() -> anyhow::Result<()> {
    office_cli::handle_command(&std::env::args().skip(1).collect::<Vec<_>>())
}
