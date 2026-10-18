# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import math

import pytest

import ols


def test_version():
    assert ols.__version__


def test_convert():
    assert math.isclose(ols.convert("length", "1000 ft", "m"), 304.8)
    assert math.isclose(ols.convert("force per length", "1 lbf/ft", "N/m"), 14.593903, rel_tol=1e-6)
    assert "temperature" in ols.quantity_kinds()


def test_errors_are_value_errors():
    with pytest.raises(ValueError, match="missing unit"):
        ols.convert("length", "12.5", "m")
    with pytest.raises(ValueError, match="unknown quantity"):
        ols.convert("volume", "1 m", "m")
