# Classifier conformance vectors

`vetores.json` is copied unchanged from TestoDoc:

| | |
|---|---|
| Source | `TestoDoc/testo-doc`, `backend/tests/fixtures/classificador/vetores.json` |
| Branch and commit | `assessment`, the commit after `06f559f` that renames two synthetic names (spec/CLASSIFIER.md §3.4) |
| sha256 | `f14676c78e9bf590e0cb1534c83ab456c82cb8bd67e696cc94bb239d891db22e` |
| Generator | `backend/app/cli/vetores_classificador.py`, Python 3.14 (Unicode 16.0.0) |
| Versions | `mascaramento-5`, `classificador-documento-1` |

All inputs are synthetic. Before the copy, a count-only check confirmed that no word used as a person's name in the file matches a word of the owner's real exam file names.

Do not edit the file by hand. To update it, regenerate it in TestoDoc, copy it here, update the sha256 above and in `tests/classifier_vectors.rs` and `spec/CLASSIFIER.md` §0.1, and run the same check again.
