use std::{borrow::Cow, collections::HashMap};

use anyhow::Result;
use helix_core::hashmap;

use crate::Editor;

pub const SPECIAL_REGISTERS: [char; 3] = ['_', '#', '.'];

type RegisterValues<'a> = Box<dyn ExactSizeIterator<Item = Cow<'a, str>> + 'a>;

pub trait Register: std::fmt::Debug {
    fn name(&self) -> char;
    fn preview(&self) -> &str;

    fn read<'a>(&'a self, editor: &'a Editor) -> RegisterValues<'a>;

    fn write(&mut self, _editor: &mut Editor, _values: Vec<String>) -> Result<()> {
        Err(anyhow::anyhow!(
            "The '{}' register is not writable",
            self.name()
        ))
    }

    fn push(&mut self, _editor: &mut Editor, _value: String) -> Result<()> {
        Err(anyhow::anyhow!(
            "The '{}' register is not writable",
            self.name()
        ))
    }
}

/// Currently just wraps a `HashMap` of `Register`s.
#[derive(Debug)]
pub struct Registers {
    inner: HashMap<char, Box<dyn Register>>,
}

impl Registers {
    pub fn get(&self, name: char) -> Option<&dyn Register> {
        self.inner.get(&name).map(AsRef::as_ref)
    }

    pub fn read<'a>(&'a self, name: char, editor: &'a Editor) -> Option<RegisterValues<'a>> {
        self.get(name).map(|reg| reg.read(editor))
    }

    pub fn write(&mut self, name: char, editor: &mut Editor, values: Vec<String>) -> Result<()> {
        if let Some(reg) = self.inner.get_mut(&name) {
            reg.write(editor, values)
        } else {
            let reg = SimpleRegister::new_with_values(name, values);
            self.inner.insert(name, Box::new(reg));
            Ok(())
        }
    }

    pub fn push(&mut self, name: char, editor: &mut Editor, value: String) -> Result<()> {
        if let Some(reg) = self.inner.get_mut(&name) {
            reg.push(editor, value)
        } else {
            self.write(name, editor, vec![value])
        }
    }

    pub fn first<'a>(&'a self, name: char, editor: &'a Editor) -> Option<Cow<'a, str>> {
        self.read(name, editor)
            .and_then(|mut entries| entries.next())
    }

    pub fn last<'a>(&'a self, name: char, editor: &'a Editor) -> Option<Cow<'a, str>> {
        self.read(name, editor).and_then(|entries| entries.last())
    }

    pub fn iter_preview(&self) -> impl Iterator<Item = (char, &str)> {
        self.inner.iter().map(|(name, reg)| (*name, reg.preview()))
    }

    pub fn clear(&mut self) {
        self.inner
            .retain(|name, _reg| !SPECIAL_REGISTERS.contains(name));
    }

    pub fn remove(&mut self, name: char) -> Option<Box<dyn Register>> {
        if SPECIAL_REGISTERS.contains(&name) {
            None
        } else {
            self.inner.remove(&name)
        }
    }
}

impl Default for Registers {
    fn default() -> Self {
        // Prepopulate the special registers.
        let inner = hashmap!(
            '_' => Box::new(BlackholeRegister::default()) as Box<dyn Register>,
            '#' => Box::new(SelectionIndexRegister::default()),
            '.' => Box::new(SelectionContentsRegister::default()),
        );

        Self { inner }
    }
}

/// A regular in-memory register.
/// This register holds values given to it with `write`/`push` and returns
/// them when read.
#[derive(Debug, Default)]
struct SimpleRegister {
    name: char,
    values: Vec<String>,
}

impl SimpleRegister {
    fn new_with_values(name: char, values: Vec<String>) -> Self {
        Self { name, values }
    }
}

impl Register for SimpleRegister {
    fn name(&self) -> char {
        self.name
    }

    fn preview(&self) -> &str {
        self.values
            .first()
            .and_then(|s| s.lines().next())
            .unwrap_or("<empty>")
    }

    fn read<'a>(&'a self, _editor: &Editor) -> RegisterValues<'a> {
        Box::new(self.values.iter().map(Cow::from))
    }

    fn write(&mut self, _editor: &mut Editor, values: Vec<String>) -> Result<()> {
        self.values = values;
        Ok(())
    }

    fn push(&mut self, _editor: &mut Editor, value: String) -> Result<()> {
        self.values.push(value);
        Ok(())
    }
}

// Special registers

/// The blackhole register discards all input and always returns nothing.
#[derive(Debug, Default)]
struct BlackholeRegister {}

impl Register for BlackholeRegister {
    fn name(&self) -> char {
        '_'
    }

    fn preview(&self) -> &str {
        "<empty>"
    }

    fn read(&self, _editor: &Editor) -> RegisterValues<'_> {
        Box::new(std::iter::empty())
    }

    fn write(&mut self, _editor: &mut Editor, _values: Vec<String>) -> Result<()> {
        Ok(())
    }

    fn push(&mut self, _editor: &mut Editor, _value: String) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Default)]
struct SelectionIndexRegister {}

impl Register for SelectionIndexRegister {
    fn name(&self) -> char {
        '#'
    }

    fn preview(&self) -> &str {
        "<selection indices>"
    }

    fn read<'a>(&self, editor: &'a Editor) -> RegisterValues<'a> {
        let (view, doc) = current_ref!(editor);
        let selections = doc.selection(view.id).len();

        // ExactSizeIterator is implemented for Range<usize> but not RangeInclusive<usize>.
        Box::new((0..selections).map(|i| i.saturating_add(1).to_string().into()))
    }
}

#[derive(Debug, Default)]
struct SelectionContentsRegister {}

impl Register for SelectionContentsRegister {
    fn name(&self) -> char {
        '.'
    }

    fn preview(&self) -> &str {
        "<selection contents>"
    }

    fn read<'a>(&'a self, editor: &'a Editor) -> RegisterValues<'a> {
        let (view, doc) = current_ref!(editor);
        let text = doc.text().slice(..);

        Box::new(doc.selection(view.id).fragments(text))
    }
}
