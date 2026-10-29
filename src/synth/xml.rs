//! The ZynAddSubFX XML format (`Misc/XMLwrapper.cpp`).
//!
//! Files are (optionally gzipped) XML with a `ZynAddSubFX-data` root.
//! Parameters are `<par name= value=>` (integers), `<par_real name=
//! value= exact_value="0x...">` (floats, the hex bits are authoritative),
//! `<par_bool name= value="yes|no">` and `<string name=>text</string>`.
//! Lookups only search direct children, like `MXML_DESCEND_FIRST`.

use std::io::Read;

use quick_xml::Reader;
use quick_xml::events::Event;

#[derive(Clone, Debug, Default)]
pub struct XmlNode {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<XmlNode>,
    pub text: String,
}

/// File version, compared like Zyn's `version_type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub i32, pub i32, pub i32);

impl XmlNode {
    pub fn new(name: &str) -> Self {
        Self { name: name.into(), ..Default::default() }
    }

    pub fn attr(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
    }

    fn find(&self, elem: &str, key: &str, val: &str) -> Option<&XmlNode> {
        self.children.iter().find(|c| c.name == elem && c.attr(key) == Some(val))
    }

    /// `enterbranch(name)`.
    pub fn branch(&self, name: &str) -> Option<&XmlNode> {
        self.children.iter().find(|c| c.name == name)
    }

    /// `enterbranch(name, id)`.
    pub fn branch_id(&self, name: &str, id: usize) -> Option<&XmlNode> {
        let s = id.to_string();
        self.find(name, "id", &s)
    }

    pub fn id(&self) -> i32 {
        self.attr("id").map(parse_int).unwrap_or(0)
    }

    pub fn par(&self, name: &str, def: i32, min: i32, max: i32) -> i32 {
        match self.find("par", "name", name).and_then(|n| n.attr("value")) {
            Some(v) => parse_int(v).clamp(min, max),
            None => def,
        }
    }

    pub fn par127(&self, name: &str, def: u8) -> u8 {
        self.par(name, def as i32, 0, 127) as u8
    }

    pub fn parbool(&self, name: &str, def: bool) -> bool {
        match self.find("par_bool", "name", name).and_then(|n| n.attr("value")) {
            Some(v) => v.starts_with('Y') || v.starts_with('y'),
            None => def,
        }
    }

    pub fn has_parreal(&self, name: &str) -> bool {
        self.find("par_real", "name", name).is_some()
    }

    pub fn parreal(&self, name: &str, def: f32) -> f32 {
        let Some(n) = self.find("par_real", "name", name) else { return def };
        if let Some(ex) = n.attr("exact_value") {
            let hex = ex.get(2..).unwrap_or("");
            if let Ok(bits) = u32::from_str_radix(hex, 16) {
                return f32::from_bits(bits);
            }
        }
        match n.attr("value") {
            Some(v) => v.trim().parse::<f32>().unwrap_or(0.0),
            None => def,
        }
    }

    pub fn parreal_lim(&self, name: &str, def: f32, min: f32, max: f32) -> f32 {
        self.parreal(name, def).clamp(min, max)
    }

    pub fn parstr(&self, name: &str) -> Option<String> {
        self.find("string", "name", name).map(|n| n.text.clone())
    }

    // ---- writing ----

    pub fn child(&mut self, name: &str) -> &mut XmlNode {
        self.children.push(XmlNode::new(name));
        self.children.last_mut().unwrap()
    }

    pub fn child_id(&mut self, name: &str, id: usize) -> &mut XmlNode {
        let mut n = XmlNode::new(name);
        n.attrs.push(("id".into(), id.to_string()));
        self.children.push(n);
        self.children.last_mut().unwrap()
    }

    fn leaf(&mut self, elem: &str, attrs: Vec<(String, String)>) {
        self.children.push(XmlNode { name: elem.into(), attrs, ..Default::default() });
    }

    pub fn add_par(&mut self, name: &str, v: i32) {
        self.leaf("par", vec![("name".into(), name.into()), ("value".into(), v.to_string())]);
    }

    pub fn add_parreal(&mut self, name: &str, v: f32) {
        self.leaf(
            "par_real",
            vec![
                ("name".into(), name.into()),
                ("value".into(), fmt_float(v)),
                ("exact_value".into(), format!("0x{:08X}", v.to_bits())),
            ],
        );
    }

    pub fn add_parbool(&mut self, name: &str, v: bool) {
        self.leaf("par_bool", vec![("name".into(), name.into()), ("value".into(), if v { "yes" } else { "no" }.into())]);
    }

    pub fn add_parstr(&mut self, name: &str, v: &str) {
        self.children.push(XmlNode {
            name: "string".into(),
            attrs: vec![("name".into(), name.into())],
            text: v.into(),
            ..Default::default()
        });
    }

    fn write(&self, out: &mut String, depth: usize) {
        for _ in 0..depth {
            out.push_str("  ");
        }
        out.push('<');
        out.push_str(&self.name);
        for (k, v) in &self.attrs {
            out.push(' ');
            out.push_str(k);
            out.push_str("=\"");
            escape(v, out);
            out.push('"');
        }
        if self.children.is_empty() && self.text.is_empty() {
            out.push_str("/>\n");
            return;
        }
        out.push('>');
        if self.children.is_empty() {
            escape(&self.text, out);
        } else {
            out.push('\n');
            for c in &self.children {
                c.write(out, depth + 1);
            }
            for _ in 0..depth {
                out.push_str("  ");
            }
        }
        out.push_str("</");
        out.push_str(&self.name);
        out.push_str(">\n");
    }
}

fn fmt_float(v: f32) -> String {
    // stringstream default: 6 significant digits.
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" { "0".into() } else { s.into() }
}

fn escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

/// `stringTo<int>`: leading integer, 0 on garbage.
fn parse_int(s: &str) -> i32 {
    let s = s.trim();
    let mut end = 0;
    for (i, c) in s.char_indices() {
        if c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+')) {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    s[..end].parse().unwrap_or(0)
}

/// A whole Zyn data file.
#[derive(Clone, Debug)]
pub struct XmlDoc {
    pub root: XmlNode,
    pub version: Version,
}

impl XmlDoc {
    /// New document with the root tag Zyn writes (we claim 3.0.7).
    pub fn new() -> Self {
        let mut root = XmlNode::new("ZynAddSubFX-data");
        root.attrs = vec![
            ("version-major".into(), "3".into()),
            ("version-minor".into(), "0".into()),
            ("version-revision".into(), "7".into()),
            ("ZynAddSubFX-author".into(), "Nasca Octavian Paul".into()),
        ];
        Self { root, version: Version(3, 0, 7) }
    }

    pub fn set_version(&mut self, a: i32, b: i32, c: i32) {
        self.version = Version(a, b, c);
        for (k, v) in self.root.attrs.iter_mut() {
            match k.as_str() {
                "version-major" => *v = a.to_string(),
                "version-minor" => *v = b.to_string(),
                "version-revision" => *v = c.to_string(),
                _ => {}
            }
        }
    }

    /// Parse plain or gzipped bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let text = if bytes.starts_with(&[0x1f, 0x8b]) {
            let mut d = flate2::read::GzDecoder::new(bytes);
            let mut s = Vec::new();
            d.read_to_end(&mut s).map_err(|e| format!("gzip: {e}"))?;
            s
        } else {
            bytes.to_vec()
        };
        let tree = parse_tree(&text)?;
        let root = find_root(&tree).ok_or("no ZynAddSubFX-data element")?.clone();
        let v = |k: &str| root.attr(k).map(parse_int).unwrap_or(0);
        let version = Version(v("version-major"), v("version-minor"), v("version-revision"));
        Ok(Self { root, version })
    }

    pub fn load_file(path: &std::path::Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&bytes)
    }

    pub fn to_string(&self) -> String {
        let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE ZynAddSubFX-data>\n");
        self.root.write(&mut s, 0);
        s
    }

    pub fn to_gzip(&self) -> Vec<u8> {
        use std::io::Write;
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let _ = e.write_all(self.to_string().as_bytes());
        e.finish().unwrap_or_default()
    }
}

impl Default for XmlDoc {
    fn default() -> Self {
        Self::new()
    }
}

fn find_root(n: &XmlNode) -> Option<&XmlNode> {
    if n.name == "ZynAddSubFX-data" {
        return Some(n);
    }
    n.children.iter().find_map(find_root)
}

fn parse_tree(text: &[u8]) -> Result<XmlNode, String> {
    let mut reader = Reader::from_reader(text);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<XmlNode> = vec![XmlNode::new("#document")];
    let mut buf = Vec::new();
    let mk = |e: &quick_xml::events::BytesStart| -> XmlNode {
        let mut n = XmlNode::new(&String::from_utf8_lossy(e.name().as_ref()));
        for a in e.attributes().flatten() {
            let k = String::from_utf8_lossy(a.key.as_ref()).into_owned();
            let v = a.unescape_value().map(|c| c.into_owned()).unwrap_or_else(|_| String::from_utf8_lossy(&a.value).into_owned());
            n.attrs.push((k, v));
        }
        n
    };
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => stack.push(mk(&e)),
            Ok(Event::Empty(e)) => {
                let n = mk(&e);
                stack.last_mut().unwrap().children.push(n);
            }
            Ok(Event::End(_)) => {
                if stack.len() > 1 {
                    let n = stack.pop().unwrap();
                    stack.last_mut().unwrap().children.push(n);
                }
            }
            Ok(Event::Text(t)) => {
                let s = t.unescape().map(|c| c.into_owned()).unwrap_or_else(|_| String::from_utf8_lossy(&t).into_owned());
                stack.last_mut().unwrap().text.push_str(&s);
            }
            Ok(Event::CData(t)) => stack.last_mut().unwrap().text.push_str(&String::from_utf8_lossy(&t)),
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                // Be lenient like mxml: keep what we have.
                if stack.len() <= 1 && stack[0].children.is_empty() {
                    return Err(format!("xml: {e}"));
                }
                break;
            }
        }
        buf.clear();
    }
    while stack.len() > 1 {
        let n = stack.pop().unwrap();
        stack.last_mut().unwrap().children.push(n);
    }
    let mut doc = stack.pop().unwrap();
    // Text nodes inside `<string>` keep whitespace; elsewhere it's noise.
    fn tidy(n: &mut XmlNode) {
        if !n.children.is_empty() {
            n.text.clear();
        }
        for c in &mut n.children {
            tidy(c);
        }
    }
    tidy(&mut doc);
    Ok(doc)
}
