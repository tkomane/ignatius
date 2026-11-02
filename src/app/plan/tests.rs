use super::*;
use crate::query::plan::PlanDocument;

fn view() -> PlanView {
    let json = r#"[{"Plan":{"Node Type":"Nested Loop","Plans":[{"Node Type":"Seq Scan","Plans":[{"Node Type":"Result"}]},{"Node Type":"Index Scan"}]}}]"#;
    let mut view = PlanView::default();
    view.ready(PlanDocument::parse(json, false).expect("fixture"));
    view
}

#[test]
fn visible_paths_are_depth_first_and_selection_moves_locally() {
    let mut view = view();
    assert_eq!(
        view.visible_paths(),
        vec![vec![], vec![0], vec![0, 0], vec![1]]
    );
    view.move_selection(1);
    assert_eq!(view.selected_path, vec![0]);
    view.move_selection(1);
    assert_eq!(view.selected_path, vec![0, 0]);
    view.move_selection(100);
    assert_eq!(view.selected_path, vec![1]);
}

#[test]
fn collapsing_hides_descendants_and_keeps_the_selection_on_the_branch() {
    let mut view = view();
    view.move_selection(1);
    view.toggle_selected();
    assert_eq!(view.visible_paths(), vec![vec![], vec![0], vec![1]]);
    assert_eq!(view.selected_path, vec![0]);
    view.toggle_selected();
    assert_eq!(view.visible_paths().len(), 4);
}

#[test]
fn selection_recovers_when_a_selected_descendant_is_hidden() {
    let mut view = view();
    view.move_selection(2);
    assert_eq!(view.selected_path, vec![0, 0]);
    view.selected_path = vec![0, 0];
    view.collapsed.insert(vec![0]);
    view.ensure_selection_visible();
    assert_eq!(view.selected_path, Vec::<usize>::new());
}

#[test]
fn leaves_have_no_expand_action() {
    let mut view = view();
    view.move_selection(2);
    view.toggle_selected();
    assert!(view.collapsed.is_empty());
}
