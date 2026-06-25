/// Reusable pagination arguments.
#[derive(clap::Args)]
pub struct Pagination {
    /// Number of results per page
    #[arg(long, default_value = "20")]
    pub limit: u32,

    /// Page number
    #[arg(long, default_value = "1")]
    pub page: u32,
}

impl Pagination {
    pub fn push_params<'a>(&'a self, params: &mut Vec<(&'a str, Option<String>)>) {
        params.push(("per_page", Some(self.limit.to_string())));
        params.push(("page", Some(self.page.to_string())));
    }
}
