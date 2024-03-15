
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Raum (String);

impl Raum {

    pub fn new(name: &str) -> Self {
        Raum(name.to_owned())
    }

    pub fn ueberschneidung(&self, other: &Self) -> bool {
        if self.0 == "Fussballfeld".to_string() && (other.0 == "Fussballfeld1".to_string() || other.0 == "Fussballfeld2".to_string()) {
            return true;
        }
        if other.0 == "Fussballfeld".to_string() && (self.0 == "Fussballfeld1".to_string() || self.0 == "Fussballfeld2".to_string()) {
            return true;
        }
        if self.0 == "Deck1".to_string() || self.0 == "Deck2".to_string() || self.0 == "Sonstiges".to_string() {
            return false;
        }

        self.0 == other.0
    }
    pub fn is(&self, s: &str) -> bool {
        self.0 == s.to_string()
    }

    pub fn get_name(&self) -> String {
        self.0.clone()
    }
}