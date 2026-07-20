"""Editor association CRUD operations mixed into the Database class."""

from __future__ import annotations

import logging

from tarragon.db._base import MixinBase

logger = logging.getLogger(__name__)


class EditorsMixin(MixinBase):
    """Manage editor command associations for file extensions."""

    def get_editor_command(self, extension: str) -> str | None:
        """Return the editor command template for an extension; None if absent."""
        logger.debug("get_editor_command: extension=%s", extension)
        row = self._execute(
            "SELECT command_template FROM editor_associations WHERE extension = ?",
            (extension,),
        ).fetchone()
        return row["command_template"] if row else None

    def upsert_editor_association(self, extension: str, command_template: str) -> None:
        """Insert or update an editor association."""
        logger.debug("upsert_editor_association: extension=%s", extension)
        self._execute(
            "INSERT OR REPLACE INTO editor_associations (extension, command_template) VALUES (?, ?)",
            (extension, command_template),
        )
        self._commit()

    def remove_editor_association(self, extension: str) -> None:
        """Remove an editor association by extension."""
        logger.debug("remove_editor_association: extension=%s", extension)
        self._execute(
            "DELETE FROM editor_associations WHERE extension = ?",
            (extension,),
        )
        self._commit()
