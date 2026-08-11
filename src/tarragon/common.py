"""Common functions and classes"""

from pathlib import Path
from typing import NamedTuple

from PIL import Image


class ImageInfo(NamedTuple):
    image: Image.Image
    path: Path | None
    width: int | None
    height: int | None
