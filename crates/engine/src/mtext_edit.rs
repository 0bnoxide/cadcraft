//! The model behind the in-place MTEXT editor: MTEXT contents as plain text with a format per
//! character and the properties of each paragraph, so a text box can show and change bold,
//! italic, underline, font, height, colour, indents and tabs and hand back MTEXT contents.
//!
//! Codes the editor has no control for (stacked fractions `\S`, `\W`, `\T`, `\Q`, `\A`, column
//! breaks `\N`, non-breaking spaces …) are kept as opaque *atoms*: one placeholder character in
//! the text ([`ATOM_BASE`] + n) that [`Formatted::to_contents`] writes back as the original
//! code. Atom identity is stored separately, so a literal private-use character stays literal.

use cadcraft_fonts::ParaProps;
pub use cadcraft_fonts::{ListStyle, MTextColor, ParaAlign, TabKind};

/// Largest paragraph indent or tab stop, in drawing units.
const MAX_INDENT: f64 = 1e6;

/// First placeholder character of the opaque codes (Unicode private use area).
pub const ATOM_BASE: u32 = 0xE000;
/// Most atoms one text keeps; further codes are dropped.
const MAX_ATOMS: usize = 1000;
/// Longest text the editor takes (characters).
pub const MAX_CHARS: usize = 100_000;
/// Most paragraphs.
const MAX_PARAS: usize = 20_000;

/// Formatting of one character.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CharFmt {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub overline: bool,
    /// Font family set inline (`\f`); `None`: the text style's.
    pub font: Option<String>,
    /// Absolute character height set inline (`\H`); `None`: the text's.
    pub height: Option<f64>,
    /// The inline height was set relative to the text's own (`\H2x;`): this multiplier, written
    /// back as it was so the part keeps scaling with the text's height.
    pub height_rel: Option<f64>,
    /// Colour set inline (`\C`, `\c`); `None`: the entity's (ByLayer/ByBlock).
    pub color: Option<MTextColor>,
}

/// A character format switch of the toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flag {
    Bold,
    Italic,
    Underline,
    Overline,
}

impl CharFmt {
    fn flag(&self, f: Flag) -> bool {
        match f {
            Flag::Bold => self.bold,
            Flag::Italic => self.italic,
            Flag::Underline => self.underline,
            Flag::Overline => self.overline,
        }
    }

    fn set_flag(&mut self, f: Flag, on: bool) {
        match f {
            Flag::Bold => self.bold = on,
            Flag::Italic => self.italic = on,
            Flag::Underline => self.underline = on,
            Flag::Overline => self.overline = on,
        }
    }
}

/// MTEXT contents as editable text.
#[derive(Clone, Debug, PartialEq)]
pub struct Formatted {
    /// The text, paragraphs separated by `\n`; atoms are placeholders.
    text: String,
    /// One format per character of `text`.
    fmt: Vec<CharFmt>,
    /// One per paragraph (`\n` count + 1).
    paras: Vec<ParaProps>,
    /// The original code of each atom.
    atoms: Vec<String>,
    /// Atom identity per character; literal text never acquires identity from its glyph.
    atom_ids: Vec<Option<usize>>,
    /// The font family bold and italic runs are written with when no font was set inline
    /// (`\fFamily|b1;`): the text style's.
    pub base_font: String,
}

impl Default for Formatted {
    fn default() -> Self {
        Formatted {
            text: String::new(),
            fmt: Vec::new(),
            paras: vec![ParaProps::default()],
            atoms: Vec::new(),
            atom_ids: Vec::new(),
            base_font: "Arial".into(),
        }
    }
}

fn arg_end(chars: &[char], from: usize) -> usize {
    let mut j = from;
    while chars.get(j).is_some_and(|c| *c != ';') {
        j += 1;
    }
    j
}

impl Formatted {
    /// Parse MTEXT contents. Never fails: what isn't understood is kept as atoms or dropped.
    pub fn from_contents(contents: &str) -> Formatted {
        Self::from_contents_with_height(contents, None)
    }

    /// [`Self::from_contents`] for a text `height` high: a relative height code (`\H2x;`) with no
    /// inline height before it scales that one instead of being dropped.
    pub fn from_contents_with_height(contents: &str, height: Option<f64>) -> Formatted {
        Self::parse(contents, height).0
    }

    /// Whether the editor holds all of `contents`: past [`MAX_CHARS`] characters, [`MAX_PARAS`]
    /// paragraphs or [`MAX_ATOMS`] kept codes the rest would be dropped, and saving would lose it.
    /// Scoped opaque codes also do not fit: their state is not represented by [`CharFmt`].
    /// Unicode escapes do not fit until the editor can consume them without hiding adjacent text.
    pub fn fits(contents: &str) -> bool {
        Self::parse(contents, None).1
    }

    /// Whether `edited`, what the editor gives back for `original` (a text `height` high), is the
    /// text as it was: the same, or what opening it and pressing OK without an edit gives (a
    /// non-canonical `{\fArial|b1;first}` comes back written another way).
    pub fn leaves_unchanged(original: &str, height: Option<f64>, edited: &str) -> bool {
        edited == original || edited == Self::from_contents_with_height(original, height).to_contents()
    }

    /// The parse, and whether it took in all of `contents`.
    fn parse(contents: &str, height: Option<f64>) -> (Formatted, bool) {
        let mut f = Formatted::default();
        let chars: Vec<char> = contents.chars().collect();
        let mut stack: Vec<CharFmt> = Vec::new();
        let mut cur = CharFmt::default();
        let mut props = ParaProps::default();
        f.paras = vec![props.clone()];
        // Cleared when a paragraph, a kept code or a nesting level doesn't fit.
        let mut whole = true;
        // Groups opened past the 64 levels kept: their closing braces pop nothing.
        let mut unkept = 0usize;
        let mut i = 0;
        while let Some(&c) = chars.get(i) {
            // Braces after the last character the editor holds add no text.
            if f.fmt.len() >= MAX_CHARS && !matches!(c, '{' | '}' | '\r') {
                return (f, false);
            }
            match c {
                '{' => {
                    if stack.len() < 64 {
                        stack.push(cur.clone());
                    } else {
                        unkept = unkept.saturating_add(1);
                        whole = false;
                    }
                    i += 1;
                }
                '}' => {
                    if unkept > 0 {
                        unkept -= 1;
                    } else if let Some(p) = stack.pop() {
                        cur = p;
                    }
                    i += 1;
                }
                '\r' => i += 1,
                '\n' => {
                    if f.paras.len() < MAX_PARAS {
                        f.push_char('\n', &cur);
                        f.paras.push(props.clone());
                    } else {
                        whole = false;
                    }
                    i += 1;
                }
                '\\' => {
                    let code = chars.get(i + 1).copied();
                    i += 2;
                    match code {
                        None => {}
                        Some('P') if f.paras.len() < MAX_PARAS => {
                            f.push_char('\n', &cur);
                            f.paras.push(props.clone());
                        }
                        Some('P') => whole = false,
                        Some(c @ ('\\' | '{' | '}')) => f.push_char(c, &cur),
                        Some('L') => cur.underline = true,
                        Some('l') => cur.underline = false,
                        Some('O') => cur.overline = true,
                        Some('o') => cur.overline = false,
                        Some(k @ ('f' | 'F' | 'H' | 'C' | 'c' | 'p')) => {
                            let end = arg_end(&chars, i);
                            let a: String = chars.get(i..end).unwrap_or_default().iter().collect();
                            i = end + 1;
                            match k {
                                'f' | 'F' => apply_font(&mut cur, &a),
                                'H' => apply_height(&mut cur, &a, height),
                                'C' => {
                                    cur.color = a.trim().parse::<u16>().ok().and_then(|v| (v > 0 && v < 256).then_some(MTextColor::Aci(v)));
                                }
                                'c' => {
                                    if let Ok(v) = a.trim().parse::<u32>() {
                                        cur.color = Some(MTextColor::Rgb(v & 0xFF_FFFF));
                                    }
                                }
                                _ => {
                                    props.apply(&a);
                                    if let Some(p) = f.paras.last_mut() {
                                        *p = props.clone();
                                    }
                                }
                            }
                        }
                        Some(k) => {
                            // Opaque formatting is not in CharFmt, so closing its group cannot
                            // restore it. Leave scoped codes to the lossless command-line editor.
                            // Unicode escapes have no semicolon; treating them as an opaque
                            // argument would also hide and potentially delete the following text.
                            whole &= stack.is_empty() && k != 'U';
                            // Any other code is kept verbatim: the ones with an argument up to
                            // their `;`, the others alone.
                            let mut code = format!("\\{k}");
                            if !matches!(k, '~' | 'K' | 'k' | 'N') {
                                let end = arg_end(&chars, i);
                                code.extend(chars.get(i..=end.min(chars.len().saturating_sub(1))).unwrap_or_default().iter());
                                i = end + 1;
                            }
                            whole &= f.atoms.len() < MAX_ATOMS;
                            f.push_atom(code, &cur);
                        }
                    }
                }
                _ => {
                    f.push_char(c, &cur);
                    i += 1;
                }
            }
        }
        (f, whole)
    }

    fn push_char(&mut self, c: char, fmt: &CharFmt) {
        self.text.push(c);
        self.fmt.push(fmt.clone());
        self.atom_ids.push(None);
    }

    fn push_atom(&mut self, code: String, fmt: &CharFmt) {
        if self.atoms.len() >= MAX_ATOMS {
            return;
        }
        let ch = char::from_u32(ATOM_BASE + self.atoms.len() as u32).unwrap_or('\u{FFFD}');
        self.atoms.push(code);
        self.push_char(ch, fmt);
        if let Some(id) = self.atom_ids.last_mut() {
            *id = Some(self.atoms.len() - 1);
        }
    }

    /// The text as the editor shows it.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Number of characters.
    pub fn len(&self) -> usize {
        self.fmt.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fmt.is_empty()
    }

    /// Number of paragraphs.
    pub fn paragraph_count(&self) -> usize {
        self.paras.len()
    }

    /// Format of character `i` (the last one past the end, the default for no text).
    pub fn fmt_at(&self, i: usize) -> CharFmt {
        self.fmt.get(i).or(self.fmt.last()).cloned().unwrap_or_default()
    }

    /// The paragraph character `i` is in.
    pub fn paragraph_of(&self, i: usize) -> usize {
        self.text.chars().take(i).filter(|c| *c == '\n').count().min(self.paras.len().saturating_sub(1))
    }

    /// Properties of paragraph `p`.
    pub fn para(&self, p: usize) -> ParaProps {
        self.paras.get(p).or(self.paras.last()).cloned().unwrap_or_default()
    }

    /// Replace a character range with literal typed or pasted text. Exact ranges distinguish
    /// an atom from an identical-looking literal beside it; whole-string diffs cannot do that.
    /// Invalid ranges or edits beyond the character/paragraph limits leave the model unchanged.
    pub fn replace_text(&mut self, range: std::ops::Range<usize>, inserted: &str, typing: &CharFmt) -> bool {
        if range.start > range.end || range.end > self.len() {
            return false;
        }
        let count = inserted.chars().take(MAX_CHARS + 1).count();
        if count > MAX_CHARS - (self.len() - range.len()) {
            return false;
        }
        let old: Vec<char> = self.text.chars().collect();
        let removed = old.get(range.clone()).unwrap_or_default();
        let first_para = old.iter().take(range.start).filter(|c| **c == '\n').count();
        let gone = removed.iter().filter(|c| **c == '\n').count();
        let added = inserted.chars().filter(|c| *c == '\n').count();
        if self.paras.len().saturating_sub(gone) + added > MAX_PARAS {
            return false;
        }
        let start = self.text.char_indices().nth(range.start).map_or(self.text.len(), |(b, _)| b);
        let end = self.text.char_indices().nth(range.end).map_or(self.text.len(), |(b, _)| b);
        let para_end = (first_para + 1 + gone).min(self.paras.len());
        self.paras.drain((first_para + 1).min(para_end)..para_end);
        let template = self.para(first_para);
        for _ in 0..added {
            self.paras.insert((first_para + 1).min(self.paras.len()), template.clone());
        }
        self.fmt.splice(range.clone(), std::iter::repeat_n(typing.clone(), count));
        self.atom_ids.splice(range, std::iter::repeat_n(None, count));
        self.text.replace_range(start..end, inserted);
        true
    }

    fn clamp(&self, r: std::ops::Range<usize>) -> std::ops::Range<usize> {
        r.start.min(self.fmt.len())..r.end.min(self.fmt.len())
    }

    /// Apply `f` to the format of the characters in `range`.
    pub fn edit(&mut self, range: std::ops::Range<usize>, f: impl Fn(&mut CharFmt)) {
        let r = self.clamp(range);
        if let Some(s) = self.fmt.get_mut(r) {
            s.iter_mut().for_each(f);
        }
    }

    /// Whether every character of `range` has `flag`.
    pub fn all_have(&self, range: std::ops::Range<usize>, flag: Flag) -> bool {
        let r = self.clamp(range);
        !r.is_empty() && self.fmt.get(r).is_some_and(|s| s.iter().all(|c| c.flag(flag)))
    }

    /// Toggle `flag` on `range`: set unless every character has it already.
    pub fn toggle(&mut self, range: std::ops::Range<usize>, flag: Flag) {
        let on = !self.all_have(range.clone(), flag);
        self.edit(range, |c| c.set_flag(flag, on));
    }

    /// The paragraphs `range` touches (a caret touches the one it is in).
    fn paras_of(&self, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
        let a = self.paragraph_of(range.start);
        let b = self.paragraph_of(range.end.max(range.start));
        a..b + 1
    }

    /// Change the properties of the paragraphs `range` touches.
    pub fn edit_paras(&mut self, range: std::ops::Range<usize>, f: impl Fn(&mut ParaProps)) {
        let ps = self.paras_of(range);
        if let Some(s) = self.paras.get_mut(ps) {
            s.iter_mut().for_each(f);
        }
    }

    /// Set the indents (in drawing units, as `\p` codes have them) of the paragraphs `range` touches: `first` is relative
    /// to `left`.
    pub fn set_indents(&mut self, range: std::ops::Range<usize>, first: f64, left: f64) {
        if first.is_finite() && left.is_finite() {
            let left = left.clamp(0.0, MAX_INDENT);
            self.edit_paras(range, |p| {
                p.first = first.clamp(-left, MAX_INDENT);
                p.left = left;
            });
        }
    }

    /// Add a tab stop at `pos` drawing units to the paragraphs `range` touches.
    pub fn add_tab(&mut self, range: std::ops::Range<usize>, pos: f64, kind: TabKind) {
        if !pos.is_finite() || !(0.0..=MAX_INDENT).contains(&pos) {
            return;
        }
        self.edit_paras(range, |p| {
            if p.tabs.len() < 64 && !p.tabs.iter().any(|(t, _)| (t - pos).abs() < 1e-6) {
                let at = p.tabs.partition_point(|(t, _)| *t < pos);
                p.tabs.insert(at, (pos, kind));
            }
        });
    }

    /// Remove the tab stop nearest `pos` (within `tolerance`) from the paragraphs `range` touches.
    pub fn remove_tab(&mut self, range: std::ops::Range<usize>, pos: f64, tolerance: f64) {
        self.edit_paras(range, |p| {
            if let Some(i) = p.tabs.iter().position(|(t, _)| (t - pos).abs() <= tolerance) {
                p.tabs.remove(i);
            }
        });
    }

    /// Make the paragraphs `range` touches a list: a hanging indent of `indent` drawing units
    /// with a tab stop there, and each paragraph a label and a tab.
    pub fn make_list(&mut self, range: std::ops::Range<usize>, style: ListStyle, indent: f64) {
        let indent = if indent.is_finite() && indent > 0.0 { indent.min(MAX_INDENT) } else { 3.0 };
        let ps = self.paras_of(range);
        let lines: Vec<&str> = self.text.split('\n').collect();
        let mut out = String::new();
        let mut fmts: Vec<CharFmt> = Vec::new();
        let mut atom_ids = Vec::new();
        let mut at = 0usize;
        for (k, line) in lines.iter().enumerate() {
            let n = line.chars().count();
            if k > 0 {
                out.push('\n');
                fmts.push(self.fmt.get(at).cloned().unwrap_or_default());
                atom_ids.push(None);
                at += 1;
            }
            if ps.contains(&k) {
                let label = format!("{}\t", style.label(k - ps.start));
                let f = self.fmt.get(at).cloned().unwrap_or_else(|| self.fmt_at(at));
                label.chars().for_each(|_| {
                    fmts.push(f.clone());
                    atom_ids.push(None);
                });
                out.push_str(&label);
            }
            out.push_str(line);
            fmts.extend(self.fmt.get(at..at + n).unwrap_or_default().iter().cloned());
            atom_ids.extend(self.atom_ids.get(at..at + n).unwrap_or_default().iter().copied());
            at += n;
        }
        if fmts.len() > MAX_CHARS {
            return;
        }
        self.text = out;
        self.fmt = fmts;
        self.atom_ids = atom_ids;
        let props = ParaProps { first: -indent, left: indent, tabs: vec![(indent, TabKind::Left)], ..ParaProps::default() };
        for p in ps {
            if let Some(q) = self.paras.get_mut(p) {
                *q = props.clone();
            }
        }
    }

    /// The MTEXT contents.
    pub fn to_contents(&self) -> String {
        let mut out = String::new();
        let mut emitted = ParaProps::default();
        let chars: Vec<char> = self.text.chars().collect();
        let mut i = 0;
        for (pi, props) in self.paras.iter().enumerate() {
            if pi > 0 {
                out.push_str("\\P");
            }
            if *props != emitted {
                out.push_str(&props.code());
                emitted = props.clone();
            }
            let end = chars.iter().skip(i).position(|c| *c == '\n').map_or(chars.len(), |n| i + n);
            while i < end {
                let f = self.fmt.get(i).cloned().unwrap_or_default();
                let mut j = i + 1;
                while j < end && self.fmt.get(j) == Some(&f) {
                    j += 1;
                }
                let run: String = chars.get(i..j).unwrap_or_default().iter().enumerate().map(|(offset, c)| self.escape(i + offset, *c)).collect();
                if f == CharFmt::default() {
                    out.push_str(&run);
                } else {
                    out.push('{');
                    out.push_str(&self.codes(&f));
                    out.push_str(&run);
                    out.push('}');
                }
                i = j;
            }
            i = end + 1;
        }
        out
    }

    /// Plain-text clipboard contents. Keep literal characters unchanged and expose opaque
    /// codes as their original source, rather than an ambiguous private-use placeholder.
    /// Pasting this text is literal; it does not recreate rich formatting.
    pub fn clipboard_text(&self, range: std::ops::Range<usize>) -> String {
        if range.start > range.end || range.end > self.len() {
            return String::new();
        }
        let mut out = String::new();
        for (at, c) in self.text.chars().enumerate().skip(range.start).take(range.len()) {
            if let Some(code) = self.atom_ids.get(at).copied().flatten().and_then(|id| self.atoms.get(id)) {
                out.push_str(code);
            } else {
                out.push(c);
            }
        }
        out
    }

    fn escape(&self, at: usize, c: char) -> String {
        if let Some(code) = self.atom_ids.get(at).copied().flatten().and_then(|id| self.atoms.get(id)) {
            return code.clone();
        }
        match c {
            '\\' | '{' | '}' => format!("\\{c}"),
            c => c.to_string(),
        }
    }

    fn codes(&self, f: &CharFmt) -> String {
        let mut s = String::new();
        if f.underline {
            s.push_str("\\L");
        }
        if f.overline {
            s.push_str("\\O");
        }
        if f.bold || f.italic || f.font.is_some() {
            let fam = f.font.clone().unwrap_or_else(|| self.base_font.clone());
            let fam: String = fam.chars().filter(|c| !matches!(c, ';' | '|' | '{' | '}' | '\\')).collect();
            s.push_str(&format!("\\f{fam}|b{}|i{};", u8::from(f.bold), u8::from(f.italic)));
        }
        if let Some(m) = f.height_rel {
            s.push_str(&format!("\\H{}x;", trim(m)));
        } else if let Some(h) = f.height {
            s.push_str(&format!("\\H{};", trim(h)));
        }
        match f.color {
            Some(MTextColor::Aci(v)) => s.push_str(&format!("\\C{v};")),
            Some(MTextColor::Rgb(v)) => s.push_str(&format!("\\c{v};")),
            None => {}
        }
        s
    }
}

fn trim(v: f64) -> String {
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A `\H` code on `cur`, for a text `base` high. A relative height over the text's own (no
/// inline height before it, or a relative one) stays relative; over an inline absolute height it
/// becomes absolute.
fn apply_height(cur: &mut CharFmt, a: &str, base: Option<f64>) {
    let valid = |h: f64| (h.is_finite() && h > 0.0 && h < 1e6).then_some(h);
    let a = a.trim();
    let Some(r) = a.strip_suffix(['x', 'X']) else {
        if let Some(h) = a.parse::<f64>().ok().and_then(valid) {
            cur.height = Some(h);
            cur.height_rel = None;
        }
        return;
    };
    let Some(m) = r.trim().parse::<f64>().ok().and_then(valid) else { return };
    match (cur.height, cur.height_rel) {
        (_, Some(r0)) => {
            if let Some(m) = valid(r0 * m) {
                cur.height_rel = Some(m);
                cur.height = base.and_then(|b| valid(b * m));
            }
        }
        (None, None) => {
            cur.height_rel = Some(m);
            cur.height = base.and_then(|b| valid(b * m));
        }
        (Some(h), None) => cur.height = valid(h * m).or(cur.height),
    }
}

fn apply_font(cur: &mut CharFmt, a: &str) {
    let mut parts = a.split('|');
    let name = parts.next().unwrap_or("").trim();
    if name.is_empty() {
        return;
    }
    cur.font = Some(name.to_string());
    cur.bold = false;
    cur.italic = false;
    for p in parts {
        match p.as_bytes() {
            [b'b', v] => cur.bold = *v == b'1',
            [b'i', v] => cur.italic = *v == b'1',
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_use_rejected_replacements_leave_every_part_unchanged() {
        let mut f = Formatted::from_contents("\\S1/2;\u{e000}\\P{\\Lunder}");
        let original = f.clone();
        let (start, end) = (2, 1);
        for range in [start..end, 0..usize::MAX, usize::MAX..usize::MAX] {
            assert!(!f.replace_text(range, "x", &CharFmt::default()));
            assert_eq!(f, original);
        }
        assert!(!f.replace_text(0..1, &"x".repeat(MAX_CHARS + 1), &CharFmt::default()));
        assert_eq!(f, original);
        assert!(!f.replace_text(0..1, &"\n".repeat(MAX_PARAS), &CharFmt::default()));
        assert_eq!(f, original);
    }

    #[test]
    fn private_use_list_labels_preserve_atom_identity_and_formats() {
        let mut f = Formatted::from_contents("\\S1/2;\u{e000}\\P{\\L\u{e3e7}}");
        f.make_list(0..f.len(), ListStyle::Number, 3.0);
        assert_eq!(f.to_contents(), "\\pxi-3,l3,t3;1.\t\\S1/2;\u{e000}\\P{\\L2.\t\u{e3e7}}");
    }

    #[test]
    fn literal_private_use_characters_round_trip_without_atoms() {
        let text: String = (0..MAX_ATOMS).filter_map(|i| char::from_u32(ATOM_BASE + i as u32)).collect();
        assert!(Formatted::fits(&text));
        assert_eq!(Formatted::from_contents(&text).to_contents(), text);
    }

    #[test]
    fn literal_private_use_characters_do_not_expand_opaque_codes() {
        let contents = "A\u{e000}\\S1/2;\u{e001}\\W2;B\u{e3e7}";
        let mut f = Formatted::from_contents(contents);
        assert!(Formatted::fits(contents));
        assert_eq!(f.to_contents(), contents);
        assert!(f.replace_text(f.len()..f.len(), "!", &CharFmt::default()));
        assert_eq!(f.to_contents(), format!("{contents}!"));
    }

    #[test]
    fn typed_private_use_characters_are_literal_even_with_atoms() {
        let mut f = Formatted::from_contents("\\S1/2; text");
        assert!(f.replace_text(f.len()..f.len(), "\u{e000}\u{e001}\u{e3e7}", &CharFmt::default()));
        assert_eq!(f.to_contents(), "\\S1/2; text\u{e000}\u{e001}\u{e3e7}");
    }

    #[test]
    fn a_relative_height_is_written_back_relative() {
        let f = Formatted::from_contents_with_height("{\\H2x;big} small {\\H3;abs}", Some(5.0));
        assert_eq!(f.fmt_at(0).height, Some(10.0), "shown at twice the text's height");
        let out = f.to_contents();
        assert!(out.contains("\\H2x;big"), "{out}");
        assert!(out.contains("\\H3;abs"), "an absolute height stays absolute: {out}");
        // Opened again for a taller text, the part scales with it.
        assert_eq!(Formatted::from_contents_with_height(&out, Some(8.0)).fmt_at(0).height, Some(16.0));
        // Relative over relative multiplies; over an absolute one it is absolute.
        let n = Formatted::from_contents_with_height("{\\H2x;a{\\H1.5x;b}}{\\H4;c{\\H2x;d}}", Some(1.0)).to_contents();
        assert!(n.contains("\\H3x;b") && n.contains("\\H8;d"), "{n}");
        // With no height known it is still kept.
        assert!(Formatted::from_contents("{\\H2x;x}").to_contents().contains("\\H2x;x"));
    }

    #[test]
    fn round_trips_formats_and_atoms() {
        for src in [
            "plain",
            "a\\Pb",
            "{\\fArial|b1|i0;bold} and {\\Lunder}",
            "x \\S1/2; y",
            "{\\H2;big}{\\C1;red}{\\c16711680;rgb}",
            "\\pxi-3,l3,t3;1.\tone\\P2.\ttwo",
            "esc \\{ \\} \\\\",
        ] {
            let f = Formatted::from_contents(src);
            let again = Formatted::from_contents(&f.to_contents());
            assert_eq!(f, again, "{src}");
        }
        assert_eq!(Formatted::from_contents("plain").to_contents(), "plain");
        assert!(Formatted::from_contents("x \\S1/2; y").to_contents().contains("\\S1/2;"));
    }

    #[test]
    fn toggles_and_typing_keep_formats() {
        let mut f = Formatted::from_contents("hello world");
        f.toggle(0..5, Flag::Bold);
        assert_eq!(f.to_contents(), "{\\fArial|b1|i0;hello} world");
        assert!(f.all_have(0..5, Flag::Bold));
        // Typing inside the bold run takes the typing format; paragraphs follow line breaks.
        let typing = f.fmt_at(2);
        assert!(f.replace_text(6..6, "big ", &CharFmt::default()));
        assert!(f.fmt_at(6) == CharFmt::default() && typing.bold);
        assert!(f.replace_text(9..10, "\n", &CharFmt::default()));
        assert_eq!(f.paragraph_count(), 2);
        assert_eq!(f.to_contents(), "{\\fArial|b1|i0;hello} big\\Pworld");
        f.toggle(0..5, Flag::Bold);
        assert_eq!(f.to_contents(), "hello big\\Pworld");
    }

    #[test]
    fn ruler_and_lists() {
        let mut f = Formatted::from_contents("one\ntwo");
        f.set_indents(0..0, -2.0, 4.0);
        f.add_tab(0..3, 4.0, TabKind::Left);
        let c = f.to_contents();
        assert!(c.starts_with("\\pxi-2,l4,t4;one"), "{c}");
        f.make_list(0..f.len(), ListStyle::Number, 3.0);
        assert_eq!(f.text(), "1.\tone\n2.\ttwo");
        assert_eq!(f.to_contents(), "\\pxi-3,l3,t3;1.\tone\\P2.\ttwo");
        f.remove_tab(0..f.len(), 3.0, 0.1);
        assert!(!f.to_contents().contains("t3"));
    }

    #[test]
    fn hostile_input_never_panics() {
        for src in
            ["\\", "{{{{", "}}}}", "\\f", "\\fArial|b", "\\H;\\H2x;\\Hnan;", "\\S", "\\p;\\pq;\\pt,,,;", "\\C999999999999;", "\u{E000}\u{E001}"]
        {
            let f = Formatted::from_contents(src);
            let _ = f.to_contents();
        }
        let mut f = Formatted::from_contents("ab");
        let (a, b) = (5, 1);
        f.edit(a..b, |c| c.bold = true);
        f.toggle(9..9, Flag::Italic);
        f.set_indents(7..9, f64::NAN, 1.0);
        f.add_tab(0..99, f64::INFINITY, TabKind::Left);
        f.make_list(50..60, ListStyle::Bullet, f64::NAN);
        assert!(!f.replace_text(0..f.len(), &"x".repeat(MAX_CHARS + 1), &CharFmt::default()));
        let _ = f.paragraph_of(usize::MAX);
        let _ = f.fmt_at(usize::MAX);
        let _ = Formatted::from_contents(&"{".repeat(100_000));
    }

    #[test]
    fn scoped_opaque_codes_use_the_lossless_command_line_editor() {
        for contents in
            [r"{\W2;wide}normal", r"{\T2;tracked}normal", r"{\Q15;slanted}normal", r"{\A1;raised}normal", r"{\Lunder{\W2;wide}under}normal"]
        {
            assert!(!Formatted::fits(contents), "unsupported scoped code: {contents}");
        }
    }

    #[test]
    fn unicode_escapes_do_not_hide_following_text_in_an_atom() {
        for contents in [r"A\U+00B0 B C;D", r"\U+00E9tail", r"\U+00E9{\W2;wide}normal", r"\U+D83D\U+DE00tail", r"\U+12 trailing", r"\U+ZZZZ trailing"]
        {
            assert!(!Formatted::fits(contents), "character escape needs lossless editing: {contents}");
        }
        assert!(Formatted::fits(r"literal \\U+00B0 text"), "an escaped backslash is literal text");
        assert!(Formatted::fits("Caf\u{e9} \u{6c34} \u{1f642}"), "literal Unicode stays editable");
    }
    #[test]
    fn modeled_scopes_and_unscoped_atoms_still_fit() {
        for contents in [r"{\Lunder}normal", r"{\H2;large}normal", r"{\C1;red}normal", r"\W2;wide", r"x \S1/2; y"] {
            assert!(Formatted::fits(contents), "supported contents: {contents}");
        }
    }
    /// A text with more paragraphs or kept codes than the editor holds doesn't fit, so TEXTEDIT
    /// leaves it to the command line instead of dropping the rest on save.
    #[test]
    fn too_many_paragraphs_or_codes_do_not_fit() {
        assert!(Formatted::fits("one\\Ptwo\\A1;three"));
        assert!(!Formatted::fits(&"a\\P".repeat(MAX_PARAS)));
        assert!(!Formatted::fits(&"a\n".repeat(MAX_PARAS)));
        assert!(!Formatted::fits(&"\\A1;".repeat(MAX_ATOMS + 1)));
        assert!(Formatted::fits(&"\\A1;".repeat(MAX_ATOMS)));
        // A full formatted run's closing brace adds no text: it still fits.
        assert!(Formatted::fits(&format!("{{\\L{}}}", "x".repeat(MAX_CHARS))));
        assert!(!Formatted::fits(&format!("{{\\L{}}}y", "x".repeat(MAX_CHARS))));
        // OK without an edit leaves a text alone, also one the editor writes another way.
        let odd = "{\\fArial|b1;first}";
        assert!(Formatted::leaves_unchanged(odd, Some(2.5), &Formatted::from_contents_with_height(odd, Some(2.5)).to_contents()));
        assert!(Formatted::leaves_unchanged(odd, Some(2.5), odd));
        assert!(!Formatted::leaves_unchanged(odd, Some(2.5), "second"));
        // Nesting past the 64 levels the editor keeps doesn't fit.
        assert!(Formatted::fits(&format!("{}a{}", "{".repeat(64), "}".repeat(64))));
        assert!(!Formatted::fits(&format!("{}a{}", "{".repeat(65), "}".repeat(65))));
    }
}
