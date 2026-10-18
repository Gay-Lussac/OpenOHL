# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""The name of the `ohl` module before 2026-10-18, kept for a while: the
same functions. Use `import ohl`."""

import warnings

from ohl import __all__, __version__, convert, quantity_kinds  # noqa: F401

warnings.warn(
    "the ols module is now called ohl: use import ohl (ols will be removed in a later version)",
    DeprecationWarning,
    stacklevel=2,
)
