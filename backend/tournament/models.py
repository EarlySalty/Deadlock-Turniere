from __future__ import annotations

from enum import Enum
from typing import Optional

from pydantic import BaseModel


# --- Enums ---

class TournamentStatus(str, Enum):
    draft = "draft"
    registration = "registration"
    group_phase = "group_phase"
    bracket = "bracket"
    completed = "completed"
    archived = "archived"


class MatchStatus(str, Enum):
    pending = "pending"
    checkin = "checkin"
    live = "live"
    completed = "completed"
    cancelled = "cancelled"


class BracketType(str, Enum):
    winners = "winners"
    losers = "losers"
    grand_final = "grand_final"


class BracketFormat(str, Enum):
    single_elimination = "single_elimination"
    double_elimination = "double_elimination"


class TeamRole(str, Enum):
    captain = "captain"
    member = "member"


class ResultSource(str, Enum):
    manual = "manual"
    automatic = "automatic"


# --- User / Session ---

class UserSession(BaseModel):
    discord_id: str
    discord_name: Optional[str] = None
    discord_avatar: Optional[str] = None
    roles: list[str] = []
    is_admin: bool = False
    is_mod: bool = False


# --- Tournament ---

class TournamentCreate(BaseModel):
    name: str
    description: Optional[str] = None
    team_size: int = 6
    bracket_format: BracketFormat = BracketFormat.single_elimination
    registration_start: Optional[str] = None
    registration_end: Optional[str] = None
    group_phase_start: Optional[str] = None
    bracket_start: Optional[str] = None


class TournamentUpdate(BaseModel):
    name: Optional[str] = None
    description: Optional[str] = None
    status: Optional[TournamentStatus] = None
    team_size: Optional[int] = None
    bracket_format: Optional[BracketFormat] = None
    registration_start: Optional[str] = None
    registration_end: Optional[str] = None
    group_phase_start: Optional[str] = None
    bracket_start: Optional[str] = None


class Tournament(BaseModel):
    id: int
    name: str
    status: TournamentStatus
    description: Optional[str] = None
    team_size: int = 6
    registration_start: Optional[str] = None
    registration_end: Optional[str] = None
    group_phase_start: Optional[str] = None
    bracket_start: Optional[str] = None
    bracket_format: str = "single_elimination"
    created_by: str
    created_at: str
    updated_at: str


# --- Team ---

class TeamCreate(BaseModel):
    name: str
    tournament_id: int


class TeamMember(BaseModel):
    id: int
    team_id: int
    discord_id: str
    discord_name: Optional[str] = None
    steam_id: Optional[str] = None
    rank: Optional[str] = None
    rank_score: int = 0
    role: TeamRole = TeamRole.member
    joined_at: str


class Team(BaseModel):
    id: int
    tournament_id: int
    name: str
    name_key: str
    captain_discord_id: str
    created_at: str
    members: list[TeamMember] = []


# --- Group Phase ---

class GroupTeam(BaseModel):
    id: int
    group_id: int
    team_id: int
    wins: int = 0
    losses: int = 0
    points: int = 0


class GroupMatch(BaseModel):
    id: int
    group_id: int
    team1_id: int
    team2_id: int
    winner_id: Optional[int] = None
    status: MatchStatus = MatchStatus.pending
    scheduled_at: Optional[str] = None
    played_at: Optional[str] = None


class Group(BaseModel):
    id: int
    tournament_id: int
    name: str
    seeding_order: int = 0
    teams: list[GroupTeam] = []
    matches: list[GroupMatch] = []


# --- Bracket ---

class BracketMatch(BaseModel):
    id: int
    tournament_id: int
    round: int
    position: int
    bracket_type: BracketType = BracketType.winners
    team1_id: Optional[int] = None
    team2_id: Optional[int] = None
    winner_id: Optional[int] = None
    status: MatchStatus = MatchStatus.pending
    steam_party_id: Optional[str] = None
    party_code: Optional[str] = None
    deadlock_match_id: Optional[str] = None
    scheduled_at: Optional[str] = None
    played_at: Optional[str] = None


# --- Match Result ---

class TournamentDetail(Tournament):
    teams: list[Team] = []
    groups: list[Group] = []
    bracket_matches: list[BracketMatch] = []


class MatchResult(BaseModel):
    id: int
    bracket_match_id: Optional[int] = None
    group_match_id: Optional[int] = None
    winning_team: Optional[int] = None
    duration_s: Optional[int] = None
    player_stats: Optional[str] = None
    source: ResultSource = ResultSource.manual
    created_at: str


# --- Check-In ---

class CheckIn(BaseModel):
    id: int
    match_type: str
    match_id: int
    team_id: int
    discord_id: str
    checked_in_at: str
