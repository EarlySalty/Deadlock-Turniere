from __future__ import annotations

from enum import Enum
from typing import Any, Optional

from pydantic import BaseModel, Field, field_validator


# --- Enums ---

class TournamentStatus(str, Enum):
    draft = "draft"
    registration = "registration"
    checkin = "checkin"
    group_phase = "group_phase"
    bracket = "bracket"
    completed = "completed"
    archived = "archived"


class MatchStatus(str, Enum):
    pending = "pending"
    checkin = "checkin"
    lobby_created = "lobby_created"
    in_progress = "in_progress"
    completed = "completed"
    forfeit = "forfeit"
    cancelled = "cancelled"


class BracketType(str, Enum):
    winners = "winners"
    losers = "losers"
    grand_final = "grand_final"


class BracketFormat(str, Enum):
    single_elimination = "single_elimination"
    double_elimination = "double_elimination"


class TournamentMode(str, Enum):
    """Automatisch bestimmter Turnier-Modus basierend auf Team-Anzahl."""
    group_stage = "group_stage"  # Group Phase + Bracket (>= 12 Teams)
    bracket_only = "bracket_only"  # Nur Bracket (< 12 Teams)


class TournamentGameMode(str, Enum):
    standard = "standard"
    mirror = "mirror"
    all_same = "all_same"
    random_heroes = "random_heroes"
    single_lane = "single_lane"


class LobbySettingsPreset(str, Enum):
    standard = "standard"
    fast_mode = "fast_mode"
    high_damage = "high_damage"
    low_gravity = "low_gravity"
    speed_mode = "speed_mode"
    glass_cannon = "glass_cannon"
    rich_start = "rich_start"
    chaos_mode = "chaos_mode"
    all_same_hero = "all_same_hero"
    immortal = "immortal"
    custom = "custom"


class TeamRole(str, Enum):
    captain = "captain"
    member = "member"


class RecruitmentStatus(str, Enum):
    open = "open"
    application = "application"
    closed = "closed"


class InviteMode(str, Enum):
    always = "always"
    window = "window"
    never = "never"


class InvitationStatus(str, Enum):
    pending = "pending"
    accepted = "accepted"
    rejected = "rejected"
    expired = "expired"


class ApplicationStatus(str, Enum):
    pending = "pending"
    accepted = "accepted"
    rejected = "rejected"


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

class TournamentBase(BaseModel):
    is_test: bool = False


class TournamentCreate(TournamentBase):
    name: str
    description: Optional[str] = None
    team_size: int = 6
    bracket_format: BracketFormat = BracketFormat.single_elimination
    series_format: int = Field(default=1)
    registration_start: Optional[str] = None
    registration_end: Optional[str] = None
    checkin_start: Optional[str] = None
    group_phase_start: Optional[str] = None
    bracket_start: Optional[str] = None
    invite_mode: InviteMode = InviteMode.always
    invite_window_start: Optional[str] = None
    invite_window_end: Optional[str] = None
    lobby_settings_preset: LobbySettingsPreset = LobbySettingsPreset.standard
    lobby_settings: Optional[dict[str, Any]] = None
    force_tournament_mode: Optional[TournamentMode] = None  # Admin-Override: erzwingt group_stage oder bracket_only
    tournament_game_mode: TournamentGameMode = TournamentGameMode.standard
    auto_lobby_enabled: bool = True
    exclude_from_leaderboard: bool = False
    reminder_offsets: list[int] = Field(default_factory=lambda: [1440, 120, 15])
    rules: Optional[str] = None

    @field_validator("series_format")
    @classmethod
    def validate_series_format(cls, v: int) -> int:
        if v not in (1, 3, 5):
            raise ValueError("series_format muss 1, 3 oder 5 sein")
        return v

    @field_validator("reminder_offsets")
    @classmethod
    def validate_reminder_offsets(cls, value: list[int]) -> list[int]:
        cleaned = sorted({int(offset) for offset in value if int(offset) >= 0}, reverse=True)
        return cleaned or [1440, 120, 15]


class TournamentUpdate(BaseModel):
    name: Optional[str] = None
    description: Optional[str] = None
    status: Optional[TournamentStatus] = None
    team_size: Optional[int] = None
    bracket_format: Optional[BracketFormat] = None
    series_format: int | None = Field(default=None)
    registration_start: Optional[str] = None
    registration_end: Optional[str] = None
    checkin_start: Optional[str] = None
    group_phase_start: Optional[str] = None
    bracket_start: Optional[str] = None
    invite_mode: Optional[InviteMode] = None
    invite_window_start: Optional[str] = None
    invite_window_end: Optional[str] = None
    lobby_settings_preset: Optional[LobbySettingsPreset] = None
    lobby_settings: Optional[dict[str, Any]] = None
    force_tournament_mode: Optional[TournamentMode] = None  # Admin-Override: erzwingt group_stage oder bracket_only
    tournament_game_mode: Optional[TournamentGameMode] = None
    auto_lobby_enabled: Optional[bool] = None
    exclude_from_leaderboard: Optional[bool] = None
    is_test: Optional[bool] = None
    reminder_offsets: Optional[list[int]] = None
    rules: Optional[str] = None

    @field_validator("series_format")
    @classmethod
    def validate_series_format(cls, v: int | None) -> int | None:
        if v is None:
            return v
        if v not in (1, 3, 5):
            raise ValueError("series_format muss 1, 3 oder 5 sein")
        return v

    @field_validator("reminder_offsets")
    @classmethod
    def validate_update_reminder_offsets(cls, value: list[int] | None) -> list[int] | None:
        if value is None:
            return value
        cleaned = sorted({int(offset) for offset in value if int(offset) >= 0}, reverse=True)
        return cleaned or [1440, 120, 15]


class TournamentSignup(BaseModel):
    id: int
    tournament_id: int
    discord_id: str
    discord_name: Optional[str] = None
    steam_id: Optional[str] = None
    rank: Optional[str] = None
    rank_score: int = 0
    team_id: Optional[int] = None
    signed_up_at: str


class Tournament(TournamentBase):
    id: int
    name: str
    status: TournamentStatus
    description: Optional[str] = None
    team_size: int = 6
    series_format: int = 1
    registration_start: Optional[str] = None
    registration_end: Optional[str] = None
    checkin_start: Optional[str] = None
    group_phase_start: Optional[str] = None
    bracket_start: Optional[str] = None
    bracket_format: str = "single_elimination"
    tournament_mode: TournamentMode  # Auto-determined oder admin-override
    tournament_game_mode: TournamentGameMode = TournamentGameMode.standard
    auto_lobby_enabled: bool = True
    created_by: str
    created_at: str
    updated_at: str
    invite_mode: InviteMode = InviteMode.always
    invite_window_start: Optional[str] = None
    invite_window_end: Optional[str] = None
    lobby_settings: Optional[str] = None
    exclude_from_leaderboard: bool = False
    reminder_offsets: list[int] = Field(default_factory=lambda: [1440, 120, 15])
    rules: Optional[str] = None

    @field_validator("reminder_offsets", mode="before")
    @classmethod
    def parse_reminder_offsets(cls, value: Any) -> list[int]:
        if value is None:
            return [1440, 120, 15]
        if isinstance(value, list):
            return [int(offset) for offset in value]
        if isinstance(value, str):
            import json

            try:
                parsed = json.loads(value)
            except ValueError:
                return [1440, 120, 15]
            if isinstance(parsed, list):
                return [int(offset) for offset in parsed]
        return [1440, 120, 15]


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
    recruitment_status: RecruitmentStatus = RecruitmentStatus.open
    members: list[TeamMember] = []


class TeamMemberPublic(BaseModel):
    id: int
    team_id: int
    discord_name: Optional[str] = None
    steam_id: Optional[str] = None
    rank: Optional[str] = None
    rank_score: int = 0
    role: TeamRole = TeamRole.member
    joined_at: str


class TeamPublic(BaseModel):
    id: int
    tournament_id: int
    name: str
    name_key: str
    members: list[TeamMemberPublic] = []
    created_at: str
    recruitment_status: RecruitmentStatus = RecruitmentStatus.open
    has_pending_applications: bool = False


# --- Group Phase ---

class GroupTeam(BaseModel):
    id: int
    group_id: int
    team_id: int
    team_name: str = ""
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
    steam_party_id: Optional[str] = None
    party_code: Optional[str] = None
    deadlock_match_id: Optional[str] = None
    match_duration_s: Optional[int] = None
    match_stats: Optional[str] = None
    hero_assignments: Optional[dict[str, Any]] = None
    scheduled_at: Optional[str] = None
    played_at: Optional[str] = None

    @field_validator("hero_assignments", mode="before")
    @classmethod
    def parse_group_match_hero_assignments(cls, value: Any) -> Optional[dict[str, Any]]:
        if value in (None, "", "null"):
            return None
        if isinstance(value, dict):
            return value
        if isinstance(value, str):
            import json

            try:
                parsed = json.loads(value)
            except ValueError:
                return None
            return parsed if isinstance(parsed, dict) else None
        return None


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
    mini_group_id: Optional[int] = None
    team1_id: Optional[int] = None
    team2_id: Optional[int] = None
    winner_id: Optional[int] = None
    status: MatchStatus = MatchStatus.pending
    steam_party_id: Optional[str] = None
    party_code: Optional[str] = None
    deadlock_match_id: Optional[str] = None
    match_duration_s: Optional[int] = None
    match_stats: Optional[str] = None
    hero_assignments: Optional[dict[str, Any]] = None
    series_wins_team1: int = 0
    series_wins_team2: int = 0
    games: list["MatchGame"] = []
    scheduled_at: Optional[str] = None
    played_at: Optional[str] = None

    @field_validator("hero_assignments", mode="before")
    @classmethod
    def parse_bracket_match_hero_assignments(cls, value: Any) -> Optional[dict[str, Any]]:
        if value in (None, "", "null"):
            return None
        if isinstance(value, dict):
            return value
        if isinstance(value, str):
            import json

            try:
                parsed = json.loads(value)
            except ValueError:
                return None
            return parsed if isinstance(parsed, dict) else None
        return None


class MatchGame(BaseModel):
    id: int
    bracket_match_id: int
    game_number: int
    status: str
    steam_party_id: str | None = None
    party_code: str | None = None
    deadlock_match_id: str | None = None
    winner_team: int | None = None
    duration_s: int | None = None
    match_stats: dict | None = None
    created_at: str
    completed_at: str | None = None


# --- Match Result ---

class BracketMiniGroup(BaseModel):
    id: int
    tournament_id: int
    round: int
    position: int
    advances_to_match_id: Optional[int] = None
    advances_to_slot: Optional[int] = None
    team_ids: list[int] = Field(default_factory=list)
    match_ids: list[int] = Field(default_factory=list)


class TournamentDetail(Tournament):
    teams: list[Team] = Field(default_factory=list)
    groups: list[Group] = Field(default_factory=list)
    bracket_matches: list[BracketMatch] = Field(default_factory=list)
    mini_groups: list[BracketMiniGroup] = Field(default_factory=list)
    signups: list[TournamentSignup] = Field(default_factory=list)


class TournamentSignupPublic(BaseModel):
    id: int
    tournament_id: int
    discord_name: Optional[str] = None
    rank: Optional[str] = None
    rank_score: int = 0
    team_id: Optional[int] = None
    signed_up_at: str


class TournamentDetailPublic(TournamentBase):
    id: int
    name: str
    status: TournamentStatus
    description: Optional[str] = None
    team_size: int = 6
    series_format: int = 1
    registration_start: Optional[str] = None
    registration_end: Optional[str] = None
    group_phase_start: Optional[str] = None
    bracket_start: Optional[str] = None
    bracket_format: str = "single_elimination"
    tournament_mode: TournamentMode  # Auto-determined oder admin-override
    tournament_game_mode: TournamentGameMode = TournamentGameMode.standard
    auto_lobby_enabled: bool = True
    created_by: str
    created_at: str
    updated_at: str
    invite_mode: InviteMode = InviteMode.always
    invite_window_start: Optional[str] = None
    invite_window_end: Optional[str] = None
    lobby_settings: Optional[str] = None
    rules: Optional[str] = None
    teams: list[TeamPublic] = Field(default_factory=list)
    groups: list[Group] = Field(default_factory=list)
    bracket_matches: list[BracketMatch] = Field(default_factory=list)
    mini_groups: list[BracketMiniGroup] = Field(default_factory=list)
    signups: list[TournamentSignupPublic] = Field(default_factory=list)


class MatchResult(BaseModel):
    id: int
    bracket_match_id: Optional[int] = None
    group_match_id: Optional[int] = None
    winning_team: Optional[int] = None
    duration_s: Optional[int] = None
    player_stats: Optional[str] = None
    source: ResultSource = ResultSource.manual
    created_at: str


class ConsentCreate(BaseModel):
    consent_version: int = 2


class ConsentStatus(BaseModel):
    has_consent: bool
    consented_at: Optional[str] = None
    consent_version: Optional[int] = None


class UserProfileUpdate(BaseModel):
    bio: Optional[str] = None
    invite_auto_accept: Optional[bool] = None
    notify_discord_dm: Optional[bool] = None
    notify_browser: Optional[bool] = None
    display_name: Optional[str] = None
    avatar_filename: Optional[str] = None
    notify_match_start: Optional[bool] = None
    notify_checkin: Optional[bool] = None
    notify_team_invite: Optional[bool] = None
    notify_tournament_news: Optional[bool] = None
    notify_registration_reminder: Optional[bool] = None


class UserProfile(BaseModel):
    discord_id: str
    bio: Optional[str] = None
    invite_auto_accept: bool = False
    notify_discord_dm: bool = True
    notify_browser: bool = False
    display_name: Optional[str] = None
    avatar_filename: Optional[str] = None
    notify_match_start: bool = True
    notify_checkin: bool = True
    notify_team_invite: bool = True
    notify_tournament_news: bool = False
    notify_registration_reminder: bool = True
    updated_at: Optional[str] = None


class TeamApplication(BaseModel):
    id: int
    team_id: int
    discord_name: str
    status: ApplicationStatus = ApplicationStatus.pending
    created_at: str


class TeamInvitation(BaseModel):
    id: int
    tournament_id: int
    team_id: int
    team_name: Optional[str] = None
    status: InvitationStatus = InvitationStatus.pending
    created_at: str
    expires_at: Optional[str] = None


class TournamentHistoryEntry(BaseModel):
    tournament_name: str
    placement: Optional[int] = None
    team_name: Optional[str] = None


class PlayerProfile(BaseModel):
    discord_name: str
    display_name: Optional[str] = None
    discord_avatar: Optional[str] = None
    avatar_filename: Optional[str] = None
    bio: Optional[str] = None
    rank: Optional[str] = None
    rank_score: int = 0
    tournaments_played: int = 0
    matches_played: int = 0
    matches_won: int = 0
    best_placement: Optional[int] = None
    total_points: int = 0
    tournament_history: list[TournamentHistoryEntry] = []


class LeaderboardEntry(BaseModel):
    rank_position: int
    discord_name: str
    rank: Optional[str] = None
    total_points: int = 0
    tournaments_played: int = 0
    matches_played: int = 0
    matches_won: int = 0
    best_placement: Optional[int] = None


# --- Check-In ---

class CheckIn(BaseModel):
    id: int
    match_type: str
    match_id: int
    team_id: int
    discord_id: str
    checked_in_at: str
