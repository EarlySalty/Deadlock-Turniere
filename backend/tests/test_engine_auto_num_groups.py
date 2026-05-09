from __future__ import annotations

import pytest

from tournament.engine import _auto_num_groups


@pytest.mark.parametrize(
    ("team_count", "expected_num_groups"),
    [
        (8, 2),
        (16, 4),
        (20, 5),
        (24, 6),
        (32, 8),
    ],
)
def test_auto_num_groups_returns_expected_best_practice_values(
    team_count: int,
    expected_num_groups: int,
):
    assert _auto_num_groups(team_count) == expected_num_groups
