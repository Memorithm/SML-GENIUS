use sml_core::BooleanWorkspace;

#[test]
fn active_indices_match_scalar_oracle_at_word_and_tail_boundaries() {
    for width in [0_usize, 1, 63, 64, 65, 127, 128, 129, 1024] {
        let mut bits = vec![false; width];
        for (index, bit) in bits.iter_mut().enumerate() {
            *bit = index % 63 == 0 || index % 101 == 0;
        }
        let workspace = BooleanWorkspace::from_bits(&bits);
        let expected: Vec<_> = bits
            .iter()
            .enumerate()
            .filter_map(|(index, enabled)| enabled.then_some(index))
            .collect();
        assert_eq!(workspace.active_indices().collect::<Vec<_>>(), expected);
        assert_eq!(workspace.active_indices().count(), expected.len());
    }
}

#[test]
fn active_indices_skip_empty_words_without_leaking_padding() {
    let mut bits = vec![false; 193];
    bits[0] = true;
    bits[192] = true;
    let workspace = BooleanWorkspace::from_bits(&bits);
    let mut active = workspace.active_indices();
    assert_eq!(active.next(), Some(0));
    assert_eq!(active.next(), Some(192));
    assert_eq!(active.next(), None);
    assert_eq!(active.next(), None);
}
