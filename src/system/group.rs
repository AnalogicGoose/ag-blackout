#[derive(Clone, Debug)]
pub struct Group {
    pub gid: u32,
    pub name: String,
}

impl Group {
    pub fn new(gid: u32, name: impl Into<String>) -> Self {
        Group {
            gid,
            name: name.into(),
        }
    }
}
