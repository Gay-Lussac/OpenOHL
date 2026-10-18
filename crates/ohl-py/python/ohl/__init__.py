# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""OpenOHL — open-source overhead line design (Python bindings).

>>> import ohl
>>> ohl.convert("length", "1000 ft", "m")
304.8
"""

from ohl._ohl import __version__, convert, quantity_kinds

__all__ = ["__version__", "convert", "quantity_kinds"]
