pub struct Tracker<T: PartialEq> {
    value: T,
}

impl<T: PartialEq> Tracker<T> {
    pub fn new(value: T) -> Self {
        Self { value }
    }

    /// Updates the tracked value. Returns `true` if the value changed.
    pub fn update(&mut self, new_value: T) -> bool {
        if self.value != new_value {
            self.value = new_value;
            true
        } else {
            false
        }
    }

    pub fn get(&self) -> &T {
        &self.value
    }
}
