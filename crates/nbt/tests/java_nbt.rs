//! Reads a file written by the real Java `NbtIo.writeCompressed`.
use nbt::Tag;

#[test]
fn reads_java_written_nbt() {
    let (_, t) = nbt::read_gzip(include_bytes!("java_written.nbt")).unwrap();
    assert_eq!(t.get("b"), Some(&Tag::Byte(-3)));
    assert_eq!(t.get("s"), Some(&Tag::Short(1234)));
    assert_eq!(t.get("i"), Some(&Tag::Int(-99999)));
    assert_eq!(t.get("l"), Some(&Tag::Long(i64::MIN + 5)));
    assert_eq!(t.get("f"), Some(&Tag::Float(1.5)));
    assert_eq!(t.get("d"), Some(&Tag::Double(-0.25)));
    assert_eq!(t.get("str").unwrap().as_str(), Some("nul\0 and \u{1F600} and é"));
    assert_eq!(t.get("ba"), Some(&Tag::ByteArray(vec![1, -2, 3])));
    assert_eq!(t.get("ia"), Some(&Tag::IntArray(vec![7, 8, 9])));
    assert_eq!(t.get("la"), Some(&Tag::LongArray(vec![1 << 40, -1])));
    assert_eq!(t.get("list"), Some(&Tag::List(3, vec![Tag::Int(4), Tag::Int(5)])));
    assert_eq!(t.get("inner").unwrap().get("k").unwrap().as_str(), Some("v"));
    // And Rust-written data reads back identically.
    let again = nbt::write_gzip("", &t).unwrap();
    assert_eq!(nbt::read_gzip(&again).unwrap().1, t);
}
