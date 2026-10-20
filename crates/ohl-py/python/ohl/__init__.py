# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

"""OpenOHL — open-source overhead line design (Python bindings).

>>> import ohl
>>> ohl.convert("length", "1000 ft", "m")
304.8
"""

from ohl._ohl import __version__, convert, quantity_kinds

__all__ = ["__version__", "convert", "quantity_kinds"]
