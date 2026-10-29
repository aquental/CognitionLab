import shutil
import sys
from pathlib import Path

import pytest

FIXTURES = Path(__file__).parent / "fixtures"
sys.path.insert(0, str(FIXTURES))

TOOLS = ("pdftotext", "pdfinfo", "pdftoppm", "pdfimages", "tesseract")

requires_tools = pytest.mark.skipif(
    any(shutil.which(t) is None for t in TOOLS),
    reason="needs poppler and tesseract (with Portuguese data) on PATH",
)


@pytest.fixture
def outside_repo(tmp_path: Path) -> Path:
    """A directory no git work tree contains (pytest's tmp_path is outside the repo)."""
    return tmp_path
