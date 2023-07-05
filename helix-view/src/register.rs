use std::{borrow::Cow, collections::HashMap};

use anyhow::Result;
use helix_core::hashmap;

use crate::{clipboard::ClipboardType, document::SCRATCH_BUFFER_NAME, Editor};

pub const SPECIAL_REGISTERS: [char; 6] = ['_', '#', '.', '%', '*', '+'];

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
            '%' => Box::new(DocumentPathRegister::default()),
            '*' => Box::new(SystemClipboardRegister::default()),
            '+' => Box::new(PrimaryClipboardRegister::default()),
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
    /// Saved selections or history values.
    /// These are stored in reverse order to make pushing new values to
    /// the "beginning" of the preview efficient.
    values: Vec<String>,
}

impl SimpleRegister {
    fn new_with_values(name: char, mut values: Vec<String>) -> Self {
        values.reverse();
        Self { name, values }
    }
}

impl Register for SimpleRegister {
    fn name(&self) -> char {
        self.name
    }

    fn preview(&self) -> &str {
        self.values
            .last()
            .and_then(|s| s.lines().next())
            .unwrap_or("<empty>")
    }

    fn read<'a>(&'a self, _editor: &Editor) -> RegisterValues<'a> {
        Box::new(self.values.iter().map(Cow::from).rev())
    }

    fn write(&mut self, _editor: &mut Editor, values: Vec<String>) -> Result<()> {
        self.values = values;
        self.values.reverse();
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

#[derive(Debug, Default)]
struct DocumentPathRegister {}

impl Register for DocumentPathRegister {
    fn name(&self) -> char {
        '%'
    }

    fn preview(&self) -> &str {
        "<document path>"
    }

    fn read<'a>(&self, editor: &'a Editor) -> RegisterValues<'a> {
        let doc = doc!(editor);

        let path = doc
            .path()
            .as_ref()
            .map(|p| p.to_string_lossy())
            .unwrap_or_else(|| SCRATCH_BUFFER_NAME.into());

        Box::new(std::iter::once(path))
    }
}

#[derive(Debug, Default)]
struct SystemClipboardRegister {
    values: Vec<String>,
}

impl Register for SystemClipboardRegister {
    fn name(&self) -> char {
        '*'
    }

    fn preview(&self) -> &str {
        "<system clipboard>"
    }

    fn read<'a>(&'a self, editor: &'a Editor) -> RegisterValues<'a> {
        read_from_clipboard(&self.values, editor, ClipboardType::Clipboard)
    }

    fn write(&mut self, editor: &mut Editor, values: Vec<String>) -> Result<()> {
        self.values = values;
        save_to_clipboard(&self.values, editor, ClipboardType::Clipboard)
    }

    fn push(&mut self, editor: &mut Editor, value: String) -> Result<()> {
        self.values.push(value);
        save_to_clipboard(&self.values, editor, ClipboardType::Clipboard)
    }
}

#[derive(Debug, Default)]
struct PrimaryClipboardRegister {
    values: Vec<String>,
}

impl Register for PrimaryClipboardRegister {
    fn name(&self) -> char {
        '+'
    }

    fn preview(&self) -> &str {
        "<primary clipboard>"
    }

    fn read<'a>(&'a self, editor: &'a Editor) -> RegisterValues<'a> {
        read_from_clipboard(&self.values, editor, ClipboardType::Selection)
    }

    fn write(&mut self, editor: &mut Editor, values: Vec<String>) -> Result<()> {
        self.values = values;
        save_to_clipboard(&self.values, editor, ClipboardType::Selection)
    }

    fn push(&mut self, editor: &mut Editor, value: String) -> Result<()> {
        self.values.push(value);
        save_to_clipboard(&self.values, editor, ClipboardType::Selection)
    }
}

fn save_to_clipboard(
    values: &[String],
    editor: &mut Editor,
    clipboard_type: ClipboardType,
) -> Result<()> {
    let line_ending = doc!(editor).line_ending;
    let joined = values.join(line_ending.as_str());

    editor
        .clipboard_provider
        .set_contents(joined, clipboard_type)
}

fn read_from_clipboard<'a>(
    saved_values: &'a [String],
    editor: &'a Editor,
    clipboard_type: ClipboardType,
) -> RegisterValues<'a> {
    match editor.clipboard_provider.get_contents(clipboard_type) {
        Ok(contents) => {
            // If we're pasting the same value that we just yanked, re-use
            // the saved values. This allows pasting multiple selections
            // even when yanked to a clipboard.
            if contents_are_saved(saved_values, editor, &contents) {
                Box::new(saved_values.iter().map(Cow::from))
            } else {
                Box::new(std::iter::once(contents.into()))
            }
        }
        Err(err) => {
            log::error!(
                "Failed to read {} clipboard: {}",
                match clipboard_type {
                    ClipboardType::Clipboard => "system",
                    ClipboardType::Selection => "primary",
                },
                err
            );

            Box::new(std::iter::empty())
        }
    }
}

fn contents_are_saved(saved_values: &[String], editor: &Editor, mut contents: &str) -> bool {
    let line_ending = doc!(editor).line_ending.as_str();
    let mut values = saved_values.iter();

    match values.next() {
        Some(first) if contents.starts_with(first) => {
            contents = &contents[first.len()..];
        }
        _ => return false,
    }

    for value in values {
        if contents.starts_with(line_ending) && contents[line_ending.len()..].starts_with(value) {
            contents = &contents[line_ending.len() + value.len()..];
        } else {
            return false;
        }
    }

    true
}
