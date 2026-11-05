//! Seam 2, the codec: what a rewrite of a song or set file would drop (ADR-0002). Tidy library
//! leaves a file alone when this finds anything.

use serviceflow_core::plan::Unkept;
use serviceflow_opensong::{unkept_in_set, unkept_in_song};
use serviceflow_testkit::opensong_data_root;

fn song(xml: &str) -> Vec<Unkept> {
    unkept_in_song(xml.as_bytes()).unwrap()
}

fn set(xml: &str) -> Vec<Unkept> {
    unkept_in_set(xml.as_bytes()).unwrap()
}

#[test]
fn every_fixture_song_and_set_rewrites_without_loss() {
    for folder in ["Songs", "Sets"] {
        for entry in std::fs::read_dir(opensong_data_root().join(folder)).unwrap() {
            let path = entry.unwrap().path();
            let bytes = std::fs::read(&path).unwrap();
            let unkept = if folder == "Songs" { unkept_in_song(&bytes) } else { unkept_in_set(&bytes) };
            assert_eq!(unkept, Ok(Vec::new()), "{}", path.display());
        }
    }
}

#[test]
fn unmodelled_elements_and_their_attributes_are_kept() {
    assert_eq!(song(r#"<song><title>T</title><capo print="true">2</capo><custom a="1"><x/></custom><lyrics>[V1]</lyrics></song>"#), []);
    // `timesig` alone is read as `time_sig`: nothing lost.
    assert_eq!(song("<song><timesig>3/4</timesig></song>"), []);
}

#[test]
fn comments_are_not_kept() {
    assert_eq!(song("<?xml version=\"1.0\"?><!-- from SongSelect --><song><title>T</title></song>"), [Unkept::Comments]);
    assert_eq!(song("<song><lyrics><!-- v1 -->[V1]</lyrics></song>"), [Unkept::Comments]);
    assert_eq!(set(r#"<set name="S"><!-- note --><slide_groups/></set>"#), [Unkept::Comments]);
}

#[test]
fn attributes_of_the_song_and_of_modelled_elements_are_not_kept() {
    assert_eq!(song(r#"<song id="7"><title lang="en">T</title></song>"#), [Unkept::Attributes("song".into()), Unkept::Attributes("title".into())]);
}

#[test]
fn a_repeated_modelled_element_is_not_kept() {
    assert_eq!(song("<song><title>One</title><title>Two</title></song>"), [Unkept::Repeated("title".into())]);
}

#[test]
fn timesig_beside_time_sig_is_not_kept() {
    assert_eq!(song("<song><time_sig>4/4</time_sig><timesig>3/4</timesig></song>"), [Unkept::OldTimeSignature]);
}

#[test]
fn elements_inside_a_text_element_and_stray_text_are_not_kept() {
    assert_eq!(song("<song><title>A <b>bold</b> title</title></song>"), [Unkept::Elements("title".into())]);
    assert_eq!(song("<song>stray<title>T</title></song>"), [Unkept::Text("song".into())]);
}

#[test]
fn what_a_set_rewrite_drops_is_found() {
    assert_eq!(set(r#"<set name="S" extra="kept"><slide_groups/><other a="1"/></set>"#), []);
    assert_eq!(set(r#"<set><slide_groups sort="no"/></set>"#), [Unkept::Attributes("slide_groups".into())]);
    assert_eq!(set("<set><slide_groups/><slide_groups/></set>"), [Unkept::Repeated("slide_groups".into())]);
    assert_eq!(
        set(r#"<set><slide_groups><slide_group type="song" name="A"><notes>one</notes><notes>two</notes></slide_group></slide_groups></set>"#),
        [Unkept::Repeated("notes".into())]
    );
    // A custom item is kept as its XML, whatever it holds.
    assert_eq!(set(r#"<set><slide_groups><slide_group type="custom" name="C" odd="1"><x/></slide_group></slide_groups></set>"#), []);
}
