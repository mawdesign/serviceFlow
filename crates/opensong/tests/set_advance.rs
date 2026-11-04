//! Seam 2: the `seconds`, `loop` and `transition` attributes of custom and image items are read,
//! written when changed, and everything else (`link`, `print`) comes back untouched.

use serviceflow_core::set::{Advance, ItemKind, ItemTransition, Set};
use serviceflow_opensong::{read_set, write_set};

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn advance(set: &Set, i: usize) -> Advance {
    match &set.items[i].kind {
        ItemKind::Custom(c) => c.advance,
        ItemKind::Image(i) => i.advance,
        _ => panic!("not a custom or image item"),
    }
}

const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<set name="T"><slide_groups>
<slide_group name="W" type="custom" seconds="10" loop="true" transition="2" link="keep me" print="true"><slides><slide><body>a</body></slide></slides></slide_group>
<slide_group name="P" type="image" seconds="oops" link="x"><slides><slide><filename>a.png</filename></slide></slides></slide_group>
</slide_groups></set>"#;

#[test]
fn the_attributes_are_read() {
    let set = read_set(XML.as_bytes()).unwrap();
    assert_eq!(advance(&set, 0), Advance { seconds: 10, looping: true, transition: ItemTransition::Off });
    assert_eq!(advance(&set, 1), Advance::default(), "an unreadable value reads as off");
}

#[test]
fn an_unchanged_item_is_written_exactly_as_read() {
    let set = read_set(XML.as_bytes()).unwrap();
    assert!(text(&write_set(&set)).contains(r#"seconds="10" loop="true" transition="2" link="keep me" print="true""#));
}

#[test]
fn changed_attributes_are_written_and_link_stays_untouched() {
    let mut set = read_set(XML.as_bytes()).unwrap();
    let ItemKind::Custom(custom) = &mut set.items[0].kind else { panic!() };
    custom.advance = Advance { seconds: 3, looping: false, transition: ItemTransition::On };
    set.items[1].notes = "touched".into();
    let out = text(&write_set(&set));
    assert!(out.contains(r#"seconds="3""#) && out.contains(r#"loop="false""#) && out.contains(r#"transition="1""#), "{out}");
    assert!(out.contains(r#"link="keep me""#) && out.contains(r#"print="true""#) && out.contains(r#"link="x""#), "{out}");
    assert!(out.contains(r#"seconds="oops""#), "an attribute the model did not change is not rewritten: {out}");
    let again = read_set(out.as_bytes()).unwrap();
    assert_eq!(advance(&again, 0), Advance { seconds: 3, looping: false, transition: ItemTransition::On });
    assert_eq!(write_set(&again), write_set(&read_set(&write_set(&again)).unwrap()), "byte-stable");
}

#[test]
fn zero_seconds_removes_the_attribute() {
    let mut set = read_set(XML.as_bytes()).unwrap();
    let ItemKind::Custom(custom) = &mut set.items[0].kind else { panic!() };
    custom.advance.seconds = 0;
    let out = text(&write_set(&set));
    assert!(!out.contains(r#"name="W" type="custom" seconds"#) && !out.contains(r#"seconds="10""#), "{out}");
}
