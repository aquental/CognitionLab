import subprocess
from pathlib import Path

import pytest

from anonimizador.terms import Term, TermsError, committable, count_patterns, load_terms


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


def test_numbers_survive_parentheses_line_breaks_and_unusual_spaces():
    # Regression (Grok G0-1): a parenthesis, a line break, a tab, or a non-breaking
    # space used to end a number, so these counted 0.
    assert Term("(11) 98765-4321").count("Tel: (11) 98765-4321") == 1
    assert Term("11987654321").count("Tel: (11) 98765-4321") == 1
    assert Term("2024.123456").count("Protocolo (2024) 123456") == 1
    cpf = Term("123.456.789-09")
    for gap in ("\n", "\u00a0", "\t", "\f", "\r", "\r\n"):
        assert cpf.count(f"CPF: 123.456.789-{gap}09") == 1, repr(gap)
    assert cpf.count("CPF: 123.456.789-\n08") == 0
    # Digits further apart are separate numbers.
    assert Term("12345").count("12 ---- 345") == 0


def test_a_labeled_number_is_matched_by_its_digits():
    # Regression (Grok G0-2): the label made the term a word term, so the bare number
    # in the document did not count.
    term = Term("CPF 123.456.789-09")
    assert term.numeric
    assert term.count("123.456.789-09") == 1
    assert term.count("Cadastro 123.456.789-09") == 1
    assert term.count("CPF: 123.456.789-09") == 1
    assert Term("Prontuario 4821-77").count("Registro: 482177") == 1


def test_a_terms_file_that_is_not_utf8_is_refused_without_its_bytes(outside_repo: Path):
    # Regression (Grok G0-4): a Latin-1 file raised UnicodeDecodeError, a traceback with
    # exit 1 ("found").
    terms = outside_repo / "terms.txt"
    terms.write_bytes("Fulano Conceição\n".encode("latin-1"))
    with pytest.raises(TermsError, match="not UTF-8") as error:
        load_terms(str(terms))
    assert "0xe7" not in str(error.value) and "position" not in str(error.value)


def test_terms_of_four_letters_or_more_are_searched_in_raw_bytes():
    # Regression (Grok G0-10): every word term under 6 letters was left out of raw
    # bytes, so a leftover short name (an old info string, UTF-16 metadata) was "ok".
    assert Term("Zuleo").searchable_in_binary
    assert Term("Rosa").searchable_in_binary
    # Three letters match binary data by chance (see RAW_MIN_LENGTH).
    assert not Term("Ana").searchable_in_binary


def test_patterns_allow_a_break_after_a_separator():
    # Regression (Grok G0-18): a number broken by a line or page break was no hint.
    assert count_patterns(["CPF 123.456.789-\n09"]) == {"cpf": 1}
    assert count_patterns(["CEP 01310-\n100"]) == {"cep": 1}
    assert count_patterns(["CPF 123.456.789-\n0"]) == {}


def test_adjacent_occurrences_of_a_short_term_all_count():
    # Regression (Grok G0-28): the space after one match was the space the next needed.
    term = Term("Zuleo")
    assert term.count("Zuleo") == 1
    assert term.count("Zuleo Zuleo") == 2
    assert term.count("Zuleo Zuleo Zuleo") == 3
    assert Term("Zulea").count("Zuleo Zuleo") == 0


def test_joins_count_each_distinct_pattern_match_once():
    # One occurrence sits in several overlapping joins (found while fixing Grok G0-38).
    joins = ["123.456.789-09A\n123.456.789-09Ab\n123.456.789-09Abc"]
    assert count_patterns([], joins) == {"cpf": 1}
    assert count_patterns([], ["123.456.789-09A\n987.654.321-00B"]) == {"cpf": 2}
    # The other readings still count repeats.
    assert count_patterns(["123.456.789-09 e 123.456.789-09"], joins) == {"cpf": 2}


def test_a_link_to_a_terms_file_git_could_commit_is_refused(tmp_path: Path):
    # Regression (Codex C0-11): only the link's own directory was checked, and the
    # link was then followed into the repository.
    repo = tmp_path / "repo"
    subprocess.run(["git", "init", "-q", str(repo)], check=True)
    target = repo / "terms.txt"
    target.write_text("Fulano Ficticio\n", encoding="utf-8")
    outside = tmp_path / "outside"
    outside.mkdir()
    (outside / "terms.txt").symlink_to(target)
    with pytest.raises(TermsError, match="inside a git work tree"):
        load_terms(str(outside / "terms.txt"))


def test_a_git_that_cannot_tell_refuses_instead_of_failing_open(tmp_path: Path, monkeypatch):
    # Codex CB-16: a failing `git rev-parse` was taken for "outside any repository".
    broken = tmp_path / "broken"
    broken.mkdir()
    (broken / ".git").write_text("gitdir: does-not-exist\n", encoding="utf-8")
    with pytest.raises(TermsError, match="git could not tell"):
        committable(str(broken / "terms.txt"))
    # An inherited GIT_DIR does not hide the repository the path is in.
    repo = tmp_path / "repo"
    subprocess.run(["git", "init", "-q", str(repo)], check=True)
    monkeypatch.setenv("GIT_DIR", str(tmp_path / "nowhere"))
    assert committable(str(repo / "terms.txt"))
    # Outside any repository, the path is not committable.
    assert not committable(str(tmp_path / "outside" / "terms.txt"))
