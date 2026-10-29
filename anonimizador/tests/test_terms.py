import subprocess
from pathlib import Path

import pytest

from anonimizador.terms import Term, TermsError, count_patterns, load_terms


def test_words_ignore_case_and_accents():
    term = Term("Beltrano da Conceição")
    assert term.count("Paciente: BELTRANO DA CONCEICAO") == 1


def test_long_terms_survive_ocr_spacing_and_line_breaks():
    term = Term("Fulano Ficticio")
    assert term.count("Pac: Fula no\nFic-ticio") == 1


def test_numbers_match_by_digits_whatever_the_separators():
    term = Term("123.456.789-09")
    assert term.numeric
    assert term.count("CPF 12345678909") == 1
    assert term.count("CPF: 123 456 789 09") == 1
    assert term.count("CPF: 123.456.789-08") == 0


def test_short_terms_need_whole_words_and_skip_binary():
    term = Term("Ana")
    assert term.count("Paciente: Ana Lima") == 1
    assert term.count("anamnese, banana") == 0
    assert not term.searchable_in_binary
    assert Term("Fulano Ficticio").searchable_in_binary
    assert Term("12345").searchable_in_binary


def test_patterns_are_counted_without_their_values():
    counts = count_patterns(["CPF 123.456.789-09, tel (11) 98765-4321", "e-mail a@b.com"])
    assert counts == {"cpf": 1, "phone": 1, "email": 1}


def test_a_terms_file_git_could_commit_is_refused(tmp_path: Path):
    subprocess.run(["git", "init", "-q", str(tmp_path)], check=True)
    terms = tmp_path / "terms.txt"
    terms.write_text("Fulano Ficticio\n", encoding="utf-8")
    with pytest.raises(TermsError, match="inside a git work tree"):
        load_terms(str(terms))
    # Ignored by git, it is accepted.
    (tmp_path / ".gitignore").write_text("terms.txt\n", encoding="utf-8")
    assert [t.raw for t in load_terms(str(terms))] == ["Fulano Ficticio"]


def test_comments_and_blank_lines_are_skipped_and_an_empty_list_is_refused(
    outside_repo: Path,
):
    terms = outside_repo / "terms.txt"
    terms.write_text("# names\n\n  Fulano Ficticio  \n", encoding="utf-8")
    assert [t.raw for t in load_terms(str(terms))] == ["Fulano Ficticio"]
    terms.write_text("# nothing\n\n", encoding="utf-8")
    with pytest.raises(TermsError, match="no term"):
        load_terms(str(terms))
