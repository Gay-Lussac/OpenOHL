# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

"""The name of the `ohl` module before 2026-10-18, kept for a while: the
same functions. Use `import ohl`."""

import warnings

from ohl import __all__, __version__, convert, quantity_kinds  # noqa: F401

warnings.warn(
    "the ols module is now called ohl: use import ohl (ols will be removed in a later version)",
    DeprecationWarning,
    stacklevel=2,
)
