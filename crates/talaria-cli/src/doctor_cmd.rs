use clap::Args;

#[derive(Debug, Clone, Args)]
pub struct DoctorArgs {
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub restart: bool,
    #[arg(long, requires = "restart")]
    pub yes: bool,
}

pub async fn run_doctor(_profile: Option<&str>, _args: DoctorArgs) -> i32 {
    99
}
