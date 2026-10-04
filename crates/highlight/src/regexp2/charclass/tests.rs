use super::*;

#[test]
fn membership_follows_regexp2() {
    let mut s = CharSet::default();
    s.add_range('a' as u32, 'z' as u32);
    assert!(s.contains('q'));
    assert!(!s.contains('Q'));
    s.add_lowercase();
    assert!(s.contains('q'));
    // `[\W\d]`: regexp2 stops at the first category that decides, so a digit is out.
    let mut w = CharSet::default();
    w.add_word(true);
    w.add_digit(false);
    assert!(!w.contains('5'));
    assert!(w.contains('-'));
    let mut sub = CharSet::default();
    sub.add_char('e');
    s.add_subtraction(sub);
    assert!(!s.contains('e'));
    assert!(CharSet::word(false).contains('é'));
    assert!(CharSet::space(false).contains('\u{A0}'));
    assert!(CharSet::digit(true).contains('x'));
    let mut m = CharSet::default();
    m.add_char('a');
    m.add_set(&CharSet::any());
    assert!(m.contains('\n'));
}

#[test]
fn lower_case_of_ranges() {
    let mut s = CharSet::default();
    s.add_range('A' as u32, 'F' as u32);
    s.add_lowercase();
    assert!(s.contains('c') && s.contains('C'));
    assert_eq!(to_lower('İ'), 'i');
    assert_eq!(to_lower('Σ'), 'σ');
}
