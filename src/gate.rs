#[derive(Copy)]
pub enum Gate {
    High,
    Low,
}

impl Clone for Gate {
    fn clone(&self) -> Gate {
        *self
    }
}
