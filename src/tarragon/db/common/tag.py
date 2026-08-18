from enum import StrEnum
from typing import Any


class TagSource(StrEnum):
    USER = "user"
    AUTO_COLOR = "auto_color"


class Tag:
    def __init__(
        self,
        id: int,
        name: str,
        source: TagSource | None = None,
        usage_count: int | None = None,
        usage_paths: set[str] | None = None,
    ) -> None:
        self._id = id
        self._name = name
        self._source = source
        self._usage_count = usage_count
        self._usage_paths = usage_paths

    def __repr__(self) -> str:
        repr: str = f"Tag({self._id}, {self._name}, {self._source}, {self._usage_count}, {self._usage_paths})"
        return repr

    def __eq__(self, other: Any) -> bool:
        if isinstance(other, Tag):
            return (
                (self._id == other.get_id())
                and (self._name == other.get_name())
                and (self._source == other.get_source())
                and (self._usage_count == other.get_usage_count())
                and (self._usage_paths == other.get_usage_paths())
            )
        if isinstance(other, str):
            return self._name == other
        else:
            return False

    def __hash__(self) -> int:
        return hash(self.__repr__())

    def __lt__(self, other: "Tag") -> bool:
        return self._name < other.get_name()

    def __gt__(self, other: "Tag") -> bool:
        return self._name > other.get_name()

    def get_id(self) -> int:
        """Returns the id of the tag"""
        return self._id

    def get_name(self) -> str:
        """Returns the name of the tag"""
        return self._name

    def get_source(self) -> TagSource | None:
        """Returns the source of the tag"""
        return self._source

    def set_source(self, source: TagSource) -> None:
        """Updates the source of the tag"""
        self._source = source

    def get_usage_count(self) -> int | None:
        """Returns the usage count"""
        return self._usage_count

    def set_usage_count(self, usage_count: int) -> None:
        """Updates the usage count"""
        self._usage_count = usage_count

    def get_usage_paths(self) -> set[str] | None:
        """Returns the file paths the tag is being used"""
        return self._usage_paths


def build_tag_from_dict(tag_raw: dict[str, Any]) -> Tag | None:
    id = tag_raw.get("id", None)
    name = tag_raw.get("name", None)
    if not id or not name:
        return None

    source = tag_raw.get("source", None)
    usage_count = tag_raw.get("usage_count", None)

    usage_paths_raw = tag_raw.get("usage_paths", None)
    usage_paths: set[str] | None = None

    if usage_paths_raw:
        usage_paths = set()
        for u in usage_paths_raw.split(","):
            usage_paths.add(u)

    return Tag(id=id, name=name, source=source, usage_count=usage_count, usage_paths=usage_paths)
