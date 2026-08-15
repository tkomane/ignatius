# Requirements quality checklist: Writing the SQL

**Created**: 2026-08-16

This is a reviewer-owned checklist for requirements quality. `[x]` means the
requirement wording was reviewed, not that the implementation is complete.

## Completeness

- [ ] CHK001 Are vertical, horizontal, word and page movement requirements all explicit? [Completeness, Spec §FR-501, Spec §FR-502]
- [ ] CHK002 Are forward, backward and word deletion requirements all explicit? [Completeness, Spec §FR-503]
- [ ] CHK003 Are undo, redo, bounded history and redo invalidation all specified? [Completeness, Spec §FR-504, Spec §FR-505]
- [ ] CHK004 Is the newline indentation rule defined for both a line start and a mid-line split? [Completeness, Spec §FR-506, Edge Cases]
- [ ] CHK005 Are focus boundaries defined for editor, results, tree and palette interactions? [Completeness, Spec §UX-503]

## Clarity and measurability

- [ ] CHK006 Is a character boundary defined precisely enough to cover multi-byte UTF-8 input? [Clarity, Spec §SEC-501]
- [ ] CHK007 Is the remembered vertical column defined in character columns rather than bytes or terminal cells? [Clarity, Spec §FR-501]
- [ ] CHK008 Is a "word" defined for identifiers, punctuation, whitespace and quoted text? [Clarity, Spec §UX-502]
- [ ] CHK009 Is "one undo step" quantified for typing, deletion, movement and replacement? [Clarity, Spec §UX-501]
- [ ] CHK010 Are the visible line count and cursor-following behavior measurable at both ends of a long buffer? [Measurability, Spec §FR-507, Spec §SC-503]

## Consistency and scenarios

- [ ] CHK011 Are Enter's meanings consistent across editor, tree, results and palette? [Consistency, Spec §UX-503]
- [ ] CHK012 Are the keyboard labels and action descriptions consistent with the focus-specific behavior? [Consistency, Spec §UX-504]
- [ ] CHK013 Are primary, alternate, exception and recovery scenarios represented for every user story? [Coverage, Gap]
- [ ] CHK014 Is the behavior after undo followed by new typing explicitly defined? [Coverage, Spec §FR-505]
- [ ] CHK015 Is an empty buffer and a one-character buffer covered without implying a special error state? [Edge Case, Spec §Edge Cases]

## Non-functional boundaries

- [ ] CHK016 Are the history memory bound and the meaning of the bound specified? [Non-Functional, Spec §FR-504]
- [ ] CHK017 Are no-persistence and no-clipboard boundaries explicit for the editor? [Security, Spec §Assumptions, Spec §Out of Scope]
- [ ] CHK018 Are ASCII, no-colour, compact and narrow presentations required to preserve the same editor meaning? [Accessibility, Spec §UX-503]
- [ ] CHK019 Is the real-terminal acceptance criterion separate from unit and layout evidence? [Traceability, Spec §SC-501]
- [ ] CHK020 Are syntax highlighting and query history explicitly excluded from this feature's acceptance boundary? [Scope, Spec §Numbering note, Spec §Out of Scope]

## Notes

Open requirement-quality questions must be resolved before the reviewer marks
the items above. The implementation defects recorded in `docs/status.md` are
evidence gaps, not checklist approvals.
