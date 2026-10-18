# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import importlib
import math

import pytest

import ohl


def test_version():
    assert ohl.__version__


def test_convert():
    assert math.isclose(ohl.convert("length", "1000 ft", "m"), 304.8)
    assert math.isclose(ohl.convert("force per length", "1 lbf/ft", "N/m"), 14.593903, rel_tol=1e-6)
    assert "temperature" in ohl.quantity_kinds()


def test_errors_are_value_errors():
    with pytest.raises(ValueError, match="missing unit"):
        ohl.convert("length", "12.5", "m")
    with pytest.raises(ValueError, match="unknown quantity"):
        ohl.convert("volume", "1 m", "m")


def test_former_name_still_imports():
    # `ols`, the module's name before 2026-10-18: the same functions, and a
    # warning to use `ohl`.
    with pytest.warns(DeprecationWarning, match="import ohl"):
        import ols

        importlib.reload(ols)
    assert ols.convert is ohl.convert
    assert ols.__version__ == ohl.__version__
