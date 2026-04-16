import { useRef, useState, type ChangeEvent } from 'react'
import { useParams, Link } from 'react-router-dom'
import {
  User, Trophy, Swords, Target, Star, Edit2, Check, X, Bell, UserCheck, Camera,
} from 'lucide-react'
import {
  usePlayerProfile, useMyProfile, useUpdateMyProfile, useConsent, useUploadProfileAvatar, useRevokeConsent,
} from '@/hooks/useTournament'
import { useAuth } from '@/hooks/useAuth'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import LoadingSpinner from '@/components/ui/LoadingSpinner'

function placementLabel(placement: number | null): string {
  if (!placement) return 'Noch kein Turnier'
  if (placement === 1) return 'Turniersieger'
  if (placement === 2) return 'Finalist'
  if (placement <= 4) return 'Halbfinalist'
  return `Platz ${placement}`
}

export default function PlayerProfile() {
  const { username: urlUsername } = useParams<{ username: string }>()
  const { user, isLoggedIn } = useAuth()

  // /profil route has no username param → use own discord_name
  const username = urlUsername ?? (isLoggedIn ? user?.discord_name : undefined)
  const isOwnProfile = isLoggedIn && (!urlUsername || user?.discord_name === urlUsername)

  const { data: profile, isLoading, isError } = usePlayerProfile(username ?? '')
  const { data: myProfile } = useMyProfile()
  const { data: consent } = useConsent()
  const updateProfile = useUpdateMyProfile()
  const revokeConsent = useRevokeConsent()

  const [editBio, setEditBio] = useState(false)
  const [bioValue, setBioValue] = useState('')
  const [showSettings, setShowSettings] = useState(false)
  const [settingsSaved, setSettingsSaved] = useState(false)
  const uploadAvatar = useUploadProfileAvatar()
  const [editName, setEditName] = useState(false)
  const [nameValue, setNameValue] = useState('')
  const [consentMessage, setConsentMessage] = useState<string | null>(null)
  const avatarInputRef = useRef<HTMLInputElement>(null)

  if (!username) {
    return <Card className="text-center py-10"><p className="text-muted">Kein Benutzername angegeben.</p></Card>
  }

  if (isLoading) return <LoadingSpinner />

  if (isError || !profile) {
    return (
      <Card className="text-center py-10">
        <User size={40} className="mx-auto text-muted mb-3" />
        <p className="text-muted">Spieler nicht gefunden.</p>
        <Link to="/" className="text-primary text-sm hover:underline mt-2 inline-block">Zur Startseite</Link>
      </Card>
    )
  }

  const handleBioSave = () => {
    updateProfile.mutate({ bio: bioValue }, {
      onSuccess: () => setEditBio(false),
    })
  }

  const handleBioEdit = () => {
    setBioValue(profile.bio ?? '')
    setEditBio(true)
  }

  const handleNameEdit = () => {
    setNameValue(myProfile?.display_name ?? profile.discord_name ?? '')
    setEditName(true)
  }

  const handleNameSave = () => {
    updateProfile.mutate({ display_name: nameValue.trim() || undefined }, {
      onSuccess: () => setEditName(false),
    })
  }

  const handleAvatarChange = (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return
    uploadAvatar.mutate(file)
  }

  const handleSettingsSave = (updates: { invite_auto_accept?: boolean; notify_discord_dm?: boolean }) => {
    updateProfile.mutate(updates, {
      onSuccess: () => {
        setSettingsSaved(true)
        setTimeout(() => setSettingsSaved(false), 2000)
      },
    })
  }

  const handleRevokeConsent = () => {
    if (!window.confirm('Deine Turnier-Einwilligung wirklich widerrufen? Danach musst du vor einer neuen Anmeldung erneut zustimmen.')) {
      return
    }
    revokeConsent.mutate(undefined, {
      onSuccess: () => {
        setConsentMessage('Einwilligung widerrufen. Für neue Turnier-Anmeldungen ist eine erneute Zustimmung erforderlich.')
      },
      onError: (error) => {
        setConsentMessage(error instanceof Error ? error.message : 'Widerruf fehlgeschlagen.')
      },
    })
  }

  const avatarUrl = profile.avatar_filename
    ? isOwnProfile
      ? `/turnier/api/avatars/${myProfile?.discord_id}`
      : `/turnier/api/avatars/by-name/${encodeURIComponent(profile.discord_name)}`
    : (profile.discord_avatar ?? null)

  return (
    <div className="space-y-6 max-w-2xl mx-auto">
      {/* Header */}
      <Card className="p-6">
        <div className="flex items-start gap-4">
          <div className="relative flex-shrink-0">
            {avatarUrl ? (
              <img
                src={avatarUrl}
                alt={profile.discord_name}
                className="w-16 h-16 rounded-full border-2 border-border"
              />
            ) : (
              <div className="w-16 h-16 rounded-full border-2 border-border bg-background/60 flex items-center justify-center">
                <User size={28} className="text-muted" />
              </div>
            )}
            {isOwnProfile && (
              <>
                <button
                  type="button"
                  onClick={() => avatarInputRef.current?.click()}
                  disabled={uploadAvatar.isPending}
                  className="absolute inset-0 rounded-full flex items-center justify-center bg-black/50 opacity-0 hover:opacity-100 transition-opacity"
                  title="Profilbild ändern"
                >
                  <Camera size={18} className="text-white" />
                </button>
                <input
                  ref={avatarInputRef}
                  type="file"
                  accept="image/jpeg,image/png,image/webp"
                  className="hidden"
                  onChange={handleAvatarChange}
                />
              </>
            )}
          </div>
          <div className="flex-1 min-w-0">
            {editName && isOwnProfile ? (
              <div className="flex items-center gap-2">
                <input
                  value={nameValue}
                  onChange={(e) => setNameValue(e.target.value)}
                  maxLength={32}
                  className="bg-background border border-border rounded px-2 py-1 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50 w-40"
                  autoFocus
                />
                <Button variant="ghost" size="sm" onClick={() => setEditName(false)}>
                  <X size={13} />
                </Button>
                <Button variant="primary" size="sm" onClick={handleNameSave} disabled={updateProfile.isPending}>
                  <Check size={13} />
                </Button>
              </div>
            ) : (
              <div className="flex items-center gap-2">
                <h1 className="text-xl font-bold text-foreground truncate">
                  {(isOwnProfile && myProfile?.display_name) ? myProfile.display_name : profile.discord_name}
                </h1>
                {isOwnProfile && (
                  <button type="button" onClick={handleNameEdit} className="text-muted hover:text-foreground transition-colors flex-shrink-0">
                    <Edit2 size={13} />
                  </button>
                )}
              </div>
            )}
            {profile.rank && (
              <span className="inline-block mt-1 text-xs px-2 py-0.5 rounded-full bg-primary/15 text-primary font-medium">
                {profile.rank}
              </span>
            )}
            <div className="flex items-center gap-1 mt-2">
              <Link to="/rangliste" className="text-xs text-muted hover:text-primary transition-colors">
                Zur Rangliste
              </Link>
            </div>
          </div>
        </div>
      </Card>

      {/* Stats */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
        <Card className="p-4 text-center">
          <Star size={18} className="mx-auto mb-1 text-primary" />
          <div className="text-2xl font-bold text-foreground">{profile.total_points}</div>
          <div className="text-xs text-muted">Punkte</div>
        </Card>
        <Card className="p-4 text-center">
          <Trophy size={18} className="mx-auto mb-1 text-yellow-400" />
          <div className="text-2xl font-bold text-foreground">{profile.tournaments_played}</div>
          <div className="text-xs text-muted">Turniere</div>
        </Card>
        <Card className="p-4 text-center">
          <Swords size={18} className="mx-auto mb-1 text-blue-400" />
          <div className="text-2xl font-bold text-foreground">{profile.matches_played}</div>
          <div className="text-xs text-muted">Matches</div>
        </Card>
        <Card className="p-4 text-center">
          <Target size={18} className="mx-auto mb-1 text-green-400" />
          <div className="text-2xl font-bold text-foreground">{profile.matches_won}</div>
          <div className="text-xs text-muted">Siege</div>
        </Card>
      </div>

      {profile.best_placement && (
        <Card className="p-4 flex items-center gap-3">
          <Trophy size={18} className="text-yellow-400 flex-shrink-0" />
          <div>
            <span className="text-sm text-muted">Bestes Ergebnis:</span>
            <span className="ml-2 font-medium text-foreground">{placementLabel(profile.best_placement)}</span>
          </div>
        </Card>
      )}

      {/* Bio */}
      <Card className="p-5 space-y-3">
        <div className="flex items-center justify-between">
          <h2 className="font-semibold text-foreground">Bio</h2>
          {isOwnProfile && !editBio && (
            <Button variant="ghost" size="sm" onClick={handleBioEdit}>
              <Edit2 size={13} />
              Bearbeiten
            </Button>
          )}
        </div>

        {editBio ? (
          <div className="space-y-2">
            <textarea
              value={bioValue}
              onChange={(e) => setBioValue(e.target.value)}
              maxLength={1000}
              rows={4}
              className="w-full bg-background border border-border rounded-lg px-3 py-2 text-sm text-foreground placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-primary/50 resize-none"
              placeholder="Schreib etwas über dich..."
            />
            <div className="flex items-center justify-between">
              <span className="text-xs text-muted">{bioValue.length}/1000</span>
              <div className="flex gap-2">
                <Button variant="ghost" size="sm" onClick={() => setEditBio(false)}>
                  <X size={13} /> Abbrechen
                </Button>
                <Button variant="primary" size="sm" onClick={handleBioSave} disabled={updateProfile.isPending}>
                  <Check size={13} /> Speichern
                </Button>
              </div>
            </div>
          </div>
        ) : (
          <p className="text-sm text-muted">
            {profile.bio ?? (isOwnProfile ? 'Noch keine Bio verfasst.' : 'Keine Bio vorhanden.')}
          </p>
        )}
      </Card>

      {/* Turnier-Historie */}
      {profile.tournament_history.length > 0 && (
        <Card className="p-5 space-y-3">
          <h2 className="font-semibold text-foreground">Turnier-Geschichte</h2>
          <div className="space-y-2">
            {profile.tournament_history.map((entry, i) => (
              <div key={i} className="flex items-center justify-between py-2 border-b border-border/50 last:border-0">
                <div>
                  <div className="text-sm font-medium text-foreground">{entry.tournament_name}</div>
                  {entry.team_name && (
                    <div className="text-xs text-muted">{entry.team_name}</div>
                  )}
                </div>
                {entry.placement && (
                  <span className="text-xs px-2 py-0.5 rounded-full bg-background border border-border text-muted">
                    {placementLabel(entry.placement)}
                  </span>
                )}
              </div>
            ))}
          </div>
        </Card>
      )}

      {/* Einstellungen (nur eigenes Profil) */}
      {isOwnProfile && (
        <Card className="p-5 space-y-4">
          <button
            className="w-full flex items-center justify-between text-left"
            onClick={() => setShowSettings(!showSettings)}
          >
            <h2 className="font-semibold text-foreground">Meine Einstellungen</h2>
            <span className="text-xs text-muted">{showSettings ? 'Ausblenden' : 'Anzeigen'}</span>
          </button>

          {showSettings && (
            <div className="space-y-4 pt-2">
              {consent && (
                <div className="space-y-3 p-3 rounded-lg bg-background/60 border border-border">
                  <div className="flex items-start gap-3">
                    <UserCheck size={16} className={`${consent.has_consent ? 'text-green-400' : 'text-amber-400'} flex-shrink-0 mt-0.5`} />
                    <div className="text-sm flex-1">
                      <div className="font-medium text-foreground">Turnier-Einwilligung</div>
                      <div className="text-muted text-xs mt-0.5">
                        {consent.has_consent
                          ? `Eingewilligt am ${new Date(consent.consented_at!).toLocaleDateString('de-DE')}`
                          : consent.consent_version
                            ? 'Einwilligung muss erneut bestätigt werden.'
                            : 'Noch nicht eingewilligt'}
                      </div>
                      <div className="text-muted text-xs mt-2 leading-relaxed">
                        Gilt für Live-Übertragungen sowie spätere Videos, Highlights und Zusammenschnitte
                        im Turnierkontext. Ein Widerruf ist nicht möglich, solange du in einem aktiven
                        Turnier angemeldet bist.
                      </div>
                    </div>
                  </div>
                  {consentMessage && (
                    <div className={`text-xs ${consent.has_consent ? 'text-green-400' : 'text-muted'}`}>
                      {consentMessage}
                    </div>
                  )}
                  {consent.has_consent && (
                    <div className="flex justify-end">
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={handleRevokeConsent}
                        disabled={revokeConsent.isPending}
                        className="text-red-400 border border-red-500/40 hover:bg-red-500/10"
                      >
                        {revokeConsent.isPending ? 'Widerruft...' : 'Einwilligung widerrufen'}
                      </Button>
                    </div>
                  )}
                </div>
              )}

              <label className="flex items-center gap-3 cursor-pointer">
                <input
                  type="checkbox"
                  checked={myProfile?.invite_auto_accept ?? false}
                  onChange={(e) => handleSettingsSave({ invite_auto_accept: e.target.checked })}
                  className="h-4 w-4 accent-primary"
                />
                <div>
                  <div className="text-sm font-medium text-foreground">Einladungen automatisch annehmen</div>
                  <div className="text-xs text-muted">Team-Einladungen werden sofort ohne Bestätigung angenommen</div>
                </div>
              </label>

              <label className="flex items-center gap-3 cursor-pointer">
                <input
                  type="checkbox"
                  checked={myProfile?.notify_discord_dm ?? true}
                  onChange={(e) => handleSettingsSave({ notify_discord_dm: e.target.checked })}
                  className="h-4 w-4 accent-primary"
                />
                <div className="flex items-center gap-2">
                  <Bell size={14} className="text-muted" />
                  <div>
                    <div className="text-sm font-medium text-foreground">Discord-DM-Benachrichtigungen</div>
                    <div className="text-xs text-muted">Bei neuen Einladungen und Bewerbungen</div>
                  </div>
                </div>
              </label>

              {settingsSaved && (
                <div className="text-xs text-green-400 flex items-center gap-1">
                  <Check size={12} /> Gespeichert
                </div>
              )}
            </div>
          )}
        </Card>
      )}
    </div>
  )
}
