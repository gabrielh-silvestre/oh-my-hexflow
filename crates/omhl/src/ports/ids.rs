use hexlog::IdGenerator;
use uuid::Uuid;

pub struct V7Ids;

impl IdGenerator for V7Ids {
    fn next(&self) -> Uuid {
        Uuid::now_v7()
    }
}

#[cfg(test)]
mod tests;
