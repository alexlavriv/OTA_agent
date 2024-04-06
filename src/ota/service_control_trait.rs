pub use mockall::*;
#[automock]
pub trait SystemControlTrait {
    fn find_process(&mut self, process_name: &str) -> Vec<usize>;
    fn kill_process(&self, pid: usize) -> Result<(), String>;
}
