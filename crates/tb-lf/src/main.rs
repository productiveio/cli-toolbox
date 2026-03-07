use clap::Parser;

#[derive(Parser)]
#[command(name = "tb-lf", version, about = "Langfuse insights CLI")]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
    println!("tb-lf v{}", tb_lf::VERSION);
}
