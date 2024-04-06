pub enum Action {
    RETRY,
    CONTINUE,
}
pub trait ServiceTrait {
    fn run(&self);
    fn run_once(&self) -> Action;
    fn start_update_both(&self);
}
