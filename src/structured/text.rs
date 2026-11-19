use super::{Error, Input, Kind, Parsed, Tree};
use std::ops::Range;

pub(super) fn parse(tree: &mut Tree<'_>, kind: Kind, raw: &str) -> Parsed<()> {
    let mut text = Text { raw, next: 0 };
    let root = tree.add(None, kind.label().to_owned())?;
    match kind {
        Kind::Jsonb => {
            json_syntax(raw)?;
            json(tree, &mut text, root)?;
        }
        Kind::Array { delimiter } => {
            let mut bounds = Vec::new();
            while text.peek() == Some(b'[') {
                text.take(b'[')?;
                if bounds.len() == super::MAX_DEPTH {
                    return Err(Error::Limit);
                }
                let (start, end) = text.until(b':')?;
                let lower = integer(&raw[start..end])?;
                let (start, end) = text.until(b']')?;
                let upper = integer(&raw[start..end])?;
                if lower > upper {
                    return Err(Error::Invalid);
                }
                bounds.push((lower, upper));
            }
            if !bounds.is_empty() {
                text.take(b'=')?;
            }
            array(tree, &mut text, root, delimiter, &bounds)?;
        }
        Kind::Range => parse_range(tree, &mut text, root)?,
        Kind::Vector | Kind::HalfVector => dense(tree, &mut text, root)?,
        Kind::SparseVector => sparse(tree, &mut text, root)?,
        Kind::Geometry | Kind::Geography => unreachable!("spatial parser owns EWKB"),
    }
    text.whitespace();
    if text.next != text.raw.len() {
        return Err(Error::Invalid);
    }
    let input = if kind == Kind::Jsonb {
        Input::Json
    } else {
        Input::Whole
    };
    tree.set_text(root, 0..raw.len(), Some(input));
    Ok(())
}

pub(super) fn validate_input(input: Input, raw: &str) -> Parsed<()> {
    let mut text = Text { raw, next: 0 };
    match input {
        Input::Whole => Ok(()),
        Input::Json => json_syntax(raw),
        Input::Coordinate => coordinate(raw),
        Input::ArrayElement(delimiter) => {
            text.pg_token(&[delimiter, b'}'], false, false)?;
            (text.next == raw.len()).then_some(()).ok_or(Error::Invalid)
        }
        Input::RangeBound => {
            text.pg_token(b",)]", true, true)?;
            (text.next == raw.len()).then_some(()).ok_or(Error::Invalid)
        }
    }
}

fn json_syntax(raw: &str) -> Parsed<()> {
    // Deserializing an ignored value verifies grammar without converting numbers.
    serde_json::from_str::<serde::de::IgnoredAny>(raw)
        .map(|_| ())
        .map_err(|_| Error::Invalid)
}

fn json(tree: &mut Tree<'_>, text: &mut Text<'_>, node: usize) -> Parsed<()> {
    text.whitespace();
    let start = text.next;
    match text.peek().ok_or(Error::Invalid)? {
        open @ (b'{' | b'[') => {
            text.next += 1;
            let object = open == b'{';
            let close = if object { b'}' } else { b']' };
            let mut count = 0;
            text.whitespace();
            if text.peek() != Some(close) {
                loop {
                    let label = if object {
                        let span = text.quoted(b'"', false)?;
                        let key: String =
                            serde_json::from_str(&text.raw[span]).map_err(|_| Error::Invalid)?;
                        format!("{key:?}")
                    } else {
                        format!("[{count}]")
                    };
                    if object {
                        text.whitespace();
                        text.take(b':')?;
                    }
                    let child = tree.add(Some(node), label)?;
                    json(tree, text, child)?;
                    count += 1;
                    text.whitespace();
                    if text.peek() != Some(b',') {
                        break;
                    }
                    text.next += 1;
                    text.whitespace();
                }
            }
            text.take(close)?;
            let noun = if object {
                "object keys"
            } else {
                "array elements"
            };
            tree.summarize(node, format!("{count} {noun}"));
        }
        b'"' => {
            text.quoted(b'"', false)?;
        }
        _ => {
            text.skip_until(b",]} \r\n\t");
        }
    }
    tree.set_text(node, start..text.next, Some(Input::Json));
    Ok(())
}

fn array(
    tree: &mut Tree<'_>,
    text: &mut Text<'_>,
    node: usize,
    delimiter: u8,
    bounds: &[(i32, i32)],
) -> Parsed<()> {
    text.take(b'{')?;
    text.whitespace();
    let lower = bounds.first().map_or(1, |(lower, _)| *lower);
    let mut count = 0_i32;
    if text.peek() != Some(b'}') {
        loop {
            let index = lower.checked_add(count).ok_or(Error::Invalid)?;
            let child = tree.add(Some(node), format!("[{index}]"))?;
            if text.peek() == Some(b'{') {
                array(tree, text, child, delimiter, bounds.get(1..).unwrap_or(&[]))?;
            } else {
                let span = text.pg_token(&[delimiter, b'}'], false, false)?;
                if text.raw[span.clone()].eq_ignore_ascii_case("NULL") {
                    tree.summarize(child, "NULL array element");
                }
                tree.set_text(child, span, Some(Input::ArrayElement(delimiter)));
            }
            count += 1;
            if text.peek() != Some(delimiter) {
                break;
            }
            text.next += 1;
            text.whitespace();
        }
    }
    text.take(b'}')?;
    text.whitespace();
    if let Some((_, upper)) = bounds.first()
        && upper
            .checked_sub(lower)
            .and_then(|size| size.checked_add(1))
            != Some(count)
    {
        return Err(Error::Invalid);
    }
    if count == 0 {
        tree.summarize(node, "empty array");
    } else {
        tree.summarize(node, format!("{count} elements, first subscript {lower}"));
    }
    Ok(())
}

fn parse_range(tree: &mut Tree<'_>, text: &mut Text<'_>, node: usize) -> Parsed<()> {
    if text.raw == "empty" {
        text.next = 5;
        tree.summarize(node, "empty range");
        return Ok(());
    }
    let lower_prefix = text.peek().ok_or(Error::Invalid)?;
    if !matches!(lower_prefix, b'[' | b'(') {
        return Err(Error::Invalid);
    }
    text.next += 1;
    let lower = text.pg_token(b",", true, true)?;
    text.take(b',')?;
    let upper = text.pg_token(b")]", true, true)?;
    let upper_suffix = text.peek().ok_or(Error::Invalid)?;
    text.next += 1;
    for (name, inclusive, span) in [
        ("Lower", lower_prefix == b'[', lower),
        ("Upper", upper_suffix == b']', upper),
    ] {
        let label = if span.is_empty() {
            format!("{name} bound")
        } else if inclusive {
            format!("{name} bound (inclusive)")
        } else {
            format!("{name} bound (exclusive)")
        };
        let child = tree.add(Some(node), label)?;
        if span.is_empty() {
            tree.summarize(child, "unbounded");
        }
        tree.set_text(child, span, Some(Input::RangeBound));
    }
    tree.summarize(node, "range");
    Ok(())
}

fn dense(tree: &mut Tree<'_>, text: &mut Text<'_>, node: usize) -> Parsed<()> {
    text.take(b'[')?;
    let mut dimensions = 0;
    loop {
        dimensions += 1;
        let child = tree.add(Some(node), format!("[{dimensions}]"))?;
        let span = text.skip_until(b",]");
        coordinate(&text.raw[span.clone()])?;
        tree.set_text(child, span, Some(Input::Coordinate));
        if text.peek() != Some(b',') {
            break;
        }
        text.next += 1;
    }
    text.take(b']')?;
    tree.summarize(node, format!("{dimensions} dimensions"));
    Ok(())
}

fn sparse(tree: &mut Tree<'_>, text: &mut Text<'_>, node: usize) -> Parsed<()> {
    text.take(b'{')?;
    let mut count = 0;
    let mut max_index = 0;
    if text.peek() != Some(b'}') {
        loop {
            let (start, end) = text.until(b':')?;
            let index = integer(&text.raw[start..end])?;
            if index <= max_index {
                return Err(Error::Invalid);
            }
            max_index = index;
            let child = tree.add(Some(node), format!("[{index}]"))?;
            let span = text.skip_until(b",}");
            coordinate(&text.raw[span.clone()])?;
            tree.set_text(child, span, Some(Input::Coordinate));
            count += 1;
            if text.peek() != Some(b',') {
                break;
            }
            text.next += 1;
        }
    }
    text.take(b'}')?;
    text.take(b'/')?;
    let dimensions = integer(&text.raw[text.next..])?;
    if dimensions <= 0 || dimensions < max_index {
        return Err(Error::Invalid);
    }
    text.next = text.raw.len();
    tree.summarize(
        node,
        format!("{dimensions} dimensions, {count} stored coordinates"),
    );
    Ok(())
}

fn coordinate(raw: &str) -> Parsed<()> {
    let trimmed = raw.trim();
    let number = trimmed.parse::<f64>().map_err(|_| Error::Invalid)?;
    if !number.is_finite()
        || !trimmed
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'.' | b'-' | b'+' | b'e' | b'E'))
    {
        return Err(Error::Invalid);
    }
    Ok(())
}

fn integer(raw: &str) -> Parsed<i32> {
    raw.trim().parse().map_err(|_| Error::Invalid)
}

struct Text<'a> {
    raw: &'a str,
    next: usize,
}

impl Text<'_> {
    fn peek(&self) -> Option<u8> {
        self.raw.as_bytes().get(self.next).copied()
    }

    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.next += 1;
        }
    }

    fn take(&mut self, byte: u8) -> Parsed<()> {
        if self.peek() != Some(byte) {
            return Err(Error::Invalid);
        }
        self.next += 1;
        Ok(())
    }

    fn skip_until(&mut self, end: &[u8]) -> Range<usize> {
        let start = self.next;
        while self.peek().is_some_and(|byte| !end.contains(&byte)) {
            self.next += 1;
        }
        start..self.next
    }

    fn until(&mut self, end: u8) -> Parsed<(usize, usize)> {
        let span = self.skip_until(&[end]);
        self.take(end)?;
        Ok((span.start, span.end))
    }

    fn quoted(&mut self, quote: u8, double_quote: bool) -> Parsed<Range<usize>> {
        let start = self.next;
        self.take(quote)?;
        loop {
            let byte = self.peek().ok_or(Error::Invalid)?;
            self.next += 1;
            match byte {
                b'\\' => {
                    self.peek().ok_or(Error::Invalid)?;
                    self.next += 1;
                }
                byte if byte == quote => {
                    if double_quote && self.peek() == Some(quote) {
                        self.next += 1;
                    } else {
                        return Ok(start..self.next);
                    }
                }
                _ => {}
            }
        }
    }

    fn pg_token(&mut self, end: &[u8], range: bool, empty: bool) -> Parsed<Range<usize>> {
        let start = self.next;
        if self.peek() == Some(b'"') {
            let span = self.quoted(b'"', range)?;
            return if self.peek().is_none_or(|byte| end.contains(&byte)) {
                Ok(span)
            } else {
                Err(Error::Invalid)
            };
        }
        while let Some(byte) = self.peek().filter(|byte| !end.contains(byte)) {
            match byte {
                b'"' => return Err(Error::Invalid),
                b'{' | b'}' if !range => return Err(Error::Invalid),
                b'\\' => {
                    self.next += 1;
                    self.peek().ok_or(Error::Invalid)?;
                }
                _ => {}
            }
            self.next += 1;
        }
        if !empty && self.next == start {
            return Err(Error::Invalid);
        }
        Ok(start..self.next)
    }
}
