"""Tag and file-tag CRUD operations mixed into the Database class."""

from __future__ import annotations

import logging

from tarragon.db._base import MixinBase, normalize_path
from tarragon.db.common.tag import Tag, TagSource, build_tag_from_dict

logger = logging.getLogger(__name__)


class TagsMixin(MixinBase):
    """Create, query, and delete tags and their file associations."""

    def ensure_tag(self, name: str, source: TagSource = TagSource.USER) -> Tag:
        """Insert a tag if it doesn't exist. Returns the tag id."""
        logger.debug("Called - name: %s, source: %s", name, source)
        cursor = self._execute(
            "INSERT INTO tags (name, source) VALUES (?, ?) ON CONFLICT(name) DO UPDATE SET name=name RETURNING id",
            (
                name,
                source,
            ),
        )
        tag_id = int(cursor.fetchone()["id"])
        return Tag(id=tag_id, name=name, usage_paths=None, source=source)

    def add_tag_to_files(self, paths: list[str], tag: Tag) -> None:
        """Associate one or more file paths with a given tag."""
        logger.debug(
            "Called - paths: %s, tag: %s",
            paths,
            tag,
        )
        self._executemany(
            "INSERT OR IGNORE INTO file_tags (path, tag_id) VALUES (?, ?)",
            [(normalize_path(p), tag.get_id()) for p in paths],
        )
        self._commit()

    def add_tag_to_file(self, path: str, tag: Tag) -> None:
        self.add_tag_to_files([path], tag)

    def get_tags_for_files(self, paths: list[str]) -> dict[str, set[Tag]]:
        """Batch lookup of tags for multiple file paths.

        Parameters
        ----------
        paths:
            List of file paths to look up.

        Returns
        -------
        dict[str, set[Tag]]
            Mapping of path -> set of Tags. Paths with no tags map
            to an empty set.
        """
        logger.debug("Called - paths: %s", paths)
        if not paths:
            return {}

        normalized = [normalize_path(p) for p in paths]
        placeholders = ", ".join("?" * len(normalized))
        rows = self.fetch_all(
            f"SELECT ft.path, t.id, t.name, t.source \
                FROM tags t \
                LEFT JOIN file_tags ft ON ft.tag_id = t.id \
                WHERE ft.path IN ({placeholders}) \
                ORDER BY t.name",
            tuple(normalized),
        )

        result: dict[str, set[Tag]] = {path: set() for path in normalized}
        for row in rows:
            tag = build_tag_from_dict(row)
            if tag:
                result[row["path"]].add(tag)
        return result

    def get_tags_for_file(self, path: str) -> set[Tag]:
        """Returns a list of tags for one file"""
        result = self.get_tags_for_files([path])

        if len(result) == 0:
            return set()

        norm_path = normalize_path(path)
        return result[norm_path]

    def get_all_tags(self) -> set[Tag]:
        """Return all tags with their usage counts.

        Returns
        -------
        set[Tag]
            A set of all the tags in the database
        """
        logger.debug("Called")
        tags: set[Tag] = set()
        tags_raw = self.fetch_all(
            """SELECT t.id, t.name, t.source, group_concat(ft.path) AS paths, COUNT(ft.path) AS usage_count
            FROM tags t
            LEFT JOIN file_tags ft ON ft.tag_id = t.id
            GROUP BY t.id, t.name
            ORDER BY t.name""",
        )

        for tag_raw in tags_raw:
            tag = build_tag_from_dict(tag_raw)
            if tag:
                tags.add(tag)
        return tags

    def remove_tag_from_files(self, paths: list[str], tag: Tag) -> None:
        """Remove file-tag associations for the given paths and tag."""
        logger.debug("Called - paths: %s, tag: %s", paths, tag)
        normalized = [normalize_path(p) for p in paths]
        placeholders = ",".join("?" * len(normalized))
        self._execute(
            f"DELETE FROM file_tags WHERE path IN ({placeholders}) AND tag_id = ?",
            (*normalized, tag.get_id()),
        )
        self._commit()

    def remove_tag_from_file(self, path: str, tag: Tag) -> None:
        self.remove_tag_from_files([path], tag)

    def delete_tag(self, tag: Tag) -> None:
        """Delete a tag from the database. Also removes all file-tag associations."""
        logger.debug("Called - tag: %s", tag)
        self._execute("DELETE FROM file_tags WHERE tag_id = ?", (tag.get_id(),))
        self._execute("DELETE FROM tags WHERE id = ?", (tag.get_id(),))
        self._commit()

    def replace_auto_color_tags(self, path: str, tags: set[Tag]) -> None:
        """Delete old auto_color tags for a path and insert new ones."""
        path = normalize_path(path)
        logger.debug("Called - path: %s, tags: %s", path, tags)
        self._execute(
            "DELETE FROM file_tags \
                WHERE tag_id IN ( \
                    SELECT t.id \
                    FROM tags t \
                    LEFT JOIN file_tags ft ON ft.tag_id = t.id \
                    WHERE path = ? AND source = ? \
                )",
            (
                path,
                TagSource.AUTO_COLOR,
            ),
        )
        if tags:
            db_tags: list[Tag] = []
            for tag in tags:
                ensured_tag = self.ensure_tag(tag.get_name())
                ensured_tag.set_source(TagSource.AUTO_COLOR)
                db_tags.append(ensured_tag)

            self._executemany(
                "INSERT OR IGNORE INTO file_tags (path, tag_id) VALUES (?, ?)",
                [(path, t.get_id()) for t in db_tags],
            )
        self._commit()
