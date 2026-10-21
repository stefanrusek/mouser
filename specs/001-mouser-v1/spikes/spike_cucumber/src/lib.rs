/// Toy domain: a desk with member computers.
#[derive(Default, Debug)]
pub struct Desk { pub name: String, pub members: Vec<String> }
impl Desk {
    pub fn create(name: &str, first: &str) -> Self { Desk { name: name.into(), members: vec![first.into()] } }
    pub fn approve_join(&mut self, who: &str) { self.members.push(who.into()); }
}
