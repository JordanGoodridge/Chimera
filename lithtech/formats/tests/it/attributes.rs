//! The attribute-file reader against specs/combat_arms/attribute_data.md,
//! "Test cases" 1 to 11 (the format's; 12 onwards are Combat Arms'
//! records, in games/combat_arms/tests/it/attribute_data.rs), on
//! documents written here.

use lithtech_formats::attributes::{Document, Value, c_atof, c_atoi};

/// "␍␊" in the spec's fixtures.
const CRLF: &str = "\r\n";

fn parse(text: &str) -> Document {
    Document::parse(text.as_bytes()).unwrap_or_else(|e| panic!("{text:?}: {e}"))
}

fn value<'a>(doc: &'a Document, section: &str, key: &str) -> &'a Value {
    doc.section(section)
        .and_then(|s| s.get(key))
        .unwrap_or_else(|| panic!("{section}.{key} missing"))
}

/// 1. Minimal.
#[test]
fn case_01_minimal() {
    let doc = parse(&format!("[A]{CRLF}k=1{CRLF}"));
    assert_eq!(doc.sections().len(), 1);
    assert_eq!(doc.sections()[0].name(), "A");
    assert_eq!(value(&doc, "A", "k"), &Value::Int(1));
}

/// 2. Every type.
#[test]
fn case_02_every_type() {
    let lines = [
        "i = -5",
        "p = +5",
        "z = 00012",
        "d = 1.5",
        "e = 1.5E-3",
        "f = -1.5e+2f",
        "w = (DWORD)4000000000",
        "b = (BYTE)300",
        "c = ( dword )7",
        "g = (FLOAT)2.25",
        "t = tRuE",
        r#"s = "a ""b"" c\d""#,
        r#"n = """#,
        "v = < 1.5 ,-2, 3e2 >",
        "q = (-1, 2)",
        "r = (1,2,3,4)",
        "h = [ -1.5 , +2 ]",
    ];
    let doc = parse(&format!("[T]{CRLF}{}{CRLF}", lines.join(CRLF)));
    let expect = [
        ("i", Value::Int(-5)),
        ("p", Value::Int(5)),
        ("z", Value::Int(12)),
        ("d", Value::Double(1.5)),
        ("e", Value::Double(0.0015)),
        ("f", Value::Float(-150.0)),
        ("w", Value::Dword(4_000_000_000)),
        ("b", Value::Byte(44)),
        ("c", Value::Dword(7)),
        ("g", Value::Float(2.25)),
        ("t", Value::Bool(true)),
        ("s", Value::Str(vec![0x61, 0x20, 0x22, 0x62, 0x22, 0x20, 0x63, 0x5C, 0x64])),
        ("n", Value::Str(Vec::new())),
        ("v", Value::Vector([1.5, -2.0, 300.0])),
        ("q", Value::Point([-1, 2])),
        ("r", Value::Rect([1, 2, 3, 4])),
        ("h", Value::Range([-1.5, 2.0])),
    ];
    for (key, want) in expect {
        assert_eq!(value(&doc, "T", key), &want, "T.{key}");
    }
}

/// 3. Comments and layout.
#[test]
fn case_03_comments_and_layout() {
    let doc = parse(&format!(r#"// head{CRLF}[A] // tail{CRLF}/* one{CRLF}two */ k = 7 /*x*/ j=8[B]m="u"//e"#));
    assert_eq!(value(&doc, "A", "k"), &Value::Int(7));
    assert_eq!(value(&doc, "A", "j"), &Value::Int(8));
    assert_eq!(value(&doc, "B", "m"), &Value::Str(b"u".to_vec()));
    // "Tokens may also touch".
    let doc = parse("[A]x=1[B]y=2");
    assert_eq!(value(&doc, "B", "y"), &Value::Int(2));
}

/// 4. Line comment ends only at LF.
#[test]
fn case_04_line_comment_ends_at_lf() {
    let doc = parse(&format!("[A]{CRLF}//c\rk=1{CRLF}"));
    assert!(doc.section("A").unwrap().is_empty());
}

/// 5. Empty documents.
#[test]
fn case_05_empty_documents() {
    assert!(Document::parse(b"").unwrap().sections().is_empty());
    assert!(parse(&format!("// x{CRLF}/* y */")).sections().is_empty());
}

/// 6. BOM and padding.
#[test]
fn case_06_bom_and_padding() {
    let body = format!("[A]{CRLF}k=\"v\"{CRLF}");
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(body.as_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    let doc = Document::parse(&bytes).unwrap();
    assert_eq!(doc, parse(&body));
    assert_eq!(value(&doc, "A", "k"), &Value::Str(b"v".to_vec()));
    // "A 0x00 byte inside the text": the document ends there.
    let doc = Document::parse(b"[A]\r\nk=1\r\n\0garbage that would not parse").unwrap();
    assert_eq!(value(&doc, "A", "k"), &Value::Int(1));
}

/// 7. Case-insensitive lookup.
#[test]
fn case_07_case_insensitive_lookup() {
    let doc = parse(&format!("[Weapon0]{CRLF}Name=\"X\"{CRLF}"));
    assert_eq!(doc.section("WEAPON0").unwrap().string("name"), Some(&b"X"[..]));
    assert_eq!(doc.section("weapon0").unwrap().string("NAME"), Some(&b"X"[..]));
}

/// 8. Duplicate key: the first one wins.
#[test]
fn case_08_duplicate_key() {
    let doc = parse(&format!("[A]{CRLF}k=1{CRLF}K=2{CRLF}"));
    assert_eq!(value(&doc, "A", "k"), &Value::Int(1));
}

/// 9. Duplicate section: rejected.
#[test]
fn case_09_duplicate_section() {
    assert!(Document::parse(format!("[A]{CRLF}k=1{CRLF}[a]{CRLF}j=2{CRLF}").as_bytes()).is_err());
}

/// 10. Rejected documents.
#[test]
fn case_10_rejected_documents() {
    let bodies = [
        "k=1e5",
        "k=.5",
        "k=5.",
        "k=- 5",
        "k=(FLOAT)2",
        "k=(INT)5",
        "k='x'",
        "k=\"a\rb\"",
        "k=<1,2>",
        "k=<1,2,3,4>",
        "k=<1.,2,3>",
        "k=(1,2,3)",
        "k=(1.5,2)",
        "k=TRUEX",
        "a.b=1",
        "a-b=1",
        "k=1/2",
        "k=1.5x",
        "# c",
        "; c",
        "/* open",
        "k=1;",
        // From "Value types": more spellings that aren't values.
        "k=12ab",
        "k=-.5",
        "k=0x10",
        "k=x",
        "k=<.5,1,1>",
        "k=<1.5f,2,3>",
        "k=(1,2,3,4,5)",
        "k=",
    ];
    for body in bodies {
        let text = format!("[A]{CRLF}{body}");
        assert!(Document::parse(text.as_bytes()).is_err(), "{body:?} accepted");
    }
    for whole in [format!("x=1{CRLF}[A]{CRLF}"), format!("[ A ]{CRLF}"), format!("[1]{CRLF}"), format!("[9a]{CRLF}")] {
        assert!(Document::parse(whole.as_bytes()).is_err(), "{whole:?} accepted");
    }
    // The error names its line.
    let e = Document::parse(format!("[A]{CRLF}k=1{CRLF}j=1e5{CRLF}").as_bytes()).unwrap_err();
    assert_eq!(e.line, 3, "{e}");
}

/// Accepted edge spellings from "Document syntax" and "Value types".
#[test]
fn accepted_spellings() {
    let doc = parse(&format!(
        "[\r\nA\r\n]{CRLF}9a=1 _1=2 x=(FLOAT)2.5f y=(byte)7 z=FALSE w=1.5E3F v=<1e3, 2, 3> [Empty]"
    ));
    let a = doc.section("A").unwrap();
    assert_eq!(a.get("9a"), Some(&Value::Int(1)));
    assert_eq!(a.get("_1"), Some(&Value::Int(2)));
    assert_eq!(a.get("x"), Some(&Value::Float(2.5)));
    assert_eq!(a.get("y"), Some(&Value::Byte(7)));
    assert_eq!(a.get("z"), Some(&Value::Bool(false)));
    assert_eq!(a.get("w"), Some(&Value::Float(1500.0)));
    assert_eq!(a.get("v"), Some(&Value::Vector([1000.0, 2.0, 3.0])));
    assert!(doc.section("empty").unwrap().is_empty());
    // Integers beyond 32 bits clamp.
    let doc = parse("[A] k=99999999999 j=-99999999999");
    assert_eq!(value(&doc, "A", "k"), &Value::Int(i32::MAX));
    assert_eq!(value(&doc, "A", "j"), &Value::Int(i32::MIN));
}

/// 11. Strict reads.
#[test]
fn case_11_strict_reads() {
    let doc = parse(&format!("[A]{CRLF}i=7{CRLF}d=7.5{CRLF}s=\"7\"{CRLF}b=TRUE{CRLF}"));
    let a = doc.section("A").unwrap();
    assert_eq!(a.int("i", -1), 7);
    assert_eq!(a.int("d", -1), -1);
    assert_eq!(a.int("s", -1), -1);
    assert_eq!(a.int("b", -1), -1);
    assert_eq!(a.number("i", -1.0), 7.0);
    assert_eq!(a.number("d", -1.0), 7.5);
    assert_eq!(a.number("s", -1.0), -1.0);
    assert_eq!(a.fixed_string("i", "fb"), "fb");
    assert_eq!(a.fixed_string("s", "fb"), "7");
    assert!(!a.bool("i", false));
    assert!(a.bool("b", false));
    assert_eq!(a.int("missing", -1), -1);
    // The other strict types.
    let doc = parse("[A] w=(DWORD)5 y=(BYTE)5 i=5");
    let a = doc.section("A").unwrap();
    assert_eq!((a.dword("w", 9), a.dword("i", 9), a.byte("y", 9), a.byte("w", 9)), (5, 9, 5, 9));
}

/// "Fixed-size string reads": 127 bytes fit, 128 give the fallback.
#[test]
fn fixed_strings_hold_127_bytes() {
    let doc = parse(&format!("[A] a=\"{}\" b=\"{}\"", "x".repeat(127), "x".repeat(128)));
    let a = doc.section("A").unwrap();
    assert_eq!(a.fixed_string("a", ""), "x".repeat(127));
    assert_eq!(a.fixed_string("b", ""), "");
}

/// "Record enumeration" and "Lists inside a record": gaps end them; a
/// string list reads non-strings as "", number lists accept integers.
#[test]
fn records_and_lists_stop_at_gaps() {
    let doc = parse("[R0] [R1] [R3] [L] s0=\"a\" s1=5 s2=\"c\" s4=\"e\" i0=1 i1=2.5 n0=1 n1=2.5 n2=\"x\"");
    assert_eq!(doc.records("R").len(), 2);
    let l = doc.section("L").unwrap();
    assert_eq!(l.string_list("s"), vec![b"a".to_vec(), Vec::new(), b"c".to_vec()]);
    assert_eq!(l.int_list("i"), vec![1, 0]);
    assert_eq!(l.number_list("n"), vec![1.0, 2.5, 0.0]);
    assert!(l.int_list("none").is_empty());
}

/// The C conversions string-typed numbers go through (band lists).
#[test]
fn c_conversions() {
    assert_eq!(c_atoi(b"3500.9"), 3500);
    assert_eq!(c_atoi(b""), 0);
    assert_eq!(c_atoi(b" -12x"), -12);
    assert_eq!(c_atof(b"0.35"), 0.35);
    assert_eq!(c_atof(b"1."), 1.0);
    assert_eq!(c_atof(b"2e2x"), 200.0);
    assert_eq!(c_atof(b"abc"), 0.0);
    assert_eq!(c_atof(b"-.5"), -0.5);
}
