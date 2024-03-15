

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Person (String);

impl Person {

    pub fn new(name: &str) -> Self {
        Person(name.to_owned())
    }
    pub fn is(&self, name: &str) -> bool {
        self.0 == name.to_string()
    }

    pub fn get_name(&self) -> String {
        self.0.clone()
    }
}