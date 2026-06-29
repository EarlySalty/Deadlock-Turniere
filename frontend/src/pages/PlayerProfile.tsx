import { useRef, useState, type ChangeEvent } from 'react'
import { useParams, Link } from 'react-router-dom'
import { motion } from 'framer-motion'
import {
  User, Trophy, Swords, Target, Star, Edit2, Check, X, Camera,
  ScrollText, Settings, ArrowUpRight
} from 'lucide-react'
import {
  usePlayerProfile, useMyProfile, useUpdateMyProfile, useUploadProfileAvatar, useRevokeConsent,
} from '@/hooks/useTournament'
import { useAuth } from '@/hooks/useAuth'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import DmSubscriptionPanel from '@/components/account/DmSubscriptionPanel'

function placementLabel(placement: number | null): string {
  if (!placement) return 'Keine Platzierung'
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
  const updateProfile = useUpdateMyProfile()
  const revokeConsent = useRevokeConsent()

  const [editBio, setEditBio] = useState(false)
  const [bioValue, setBioValue] = useState('')
  const [settingsSaved, setSettingsSaved] = useState(false)
  const uploadAvatar = useUploadProfileAvatar()
  const [editName, setEditName] = useState(false)
  const [nameValue, setNameValue] = useState('')
  const [consentMessage, setConsentMessage] = useState<string | null>(null)
  const avatarInputRef = useRef<HTMLInputElement>(null)

  if (!username) {
    return <Card className="text-center py-20 opacity-60"><p className="text-muted italic">Kein Benutzername im Archiv gefunden.</p></Card>
  }

  if (isLoading) return <LoadingSpinner />

  if (isError || !profile) {
    return (
      <Card className="text-center py-20 opacity-60">
        <User size={48} className="mx-auto text-muted mb-4 opacity-20" />
        <p className="text-muted italic">Dieser Held existiert nur in Legenden.</p>
        <Link to="/" className="text-primary text-xs font-bold uppercase tracking-widest hover:underline mt-6 inline-block">Zurück zur Arena</Link>
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

  const handleSettingsSave = (updates: {
    invite_auto_accept?: boolean
    notify_discord_dm?: boolean
    notify_registration_reminder?: boolean
  }) => {
    updateProfile.mutate(updates, {
      onSuccess: () => {
        setSettingsSaved(true)
        setTimeout(() => setSettingsSaved(false), 2000)
      },
    })
  }

  const handleRevokeConsent = () => {
    if (!window.confirm('Deine Turnier-Einwilligung wirklich widerrufen?')) {
      return
    }
    revokeConsent.mutate(undefined, {
      onSuccess: () => {
        setConsentMessage('Einwilligung widerrufen.')
      },
    })
  }

  const avatarUrl = profile.avatar_filename
    ? isOwnProfile
      ? `/turnier/api/avatars/${myProfile?.discord_id}`
      : `/turnier/api/avatars/by-name/${encodeURIComponent(profile.discord_name)}`
    : (profile.discord_avatar ?? null)

  return (
    <motion.div
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      className="space-y-8 max-w-4xl mx-auto pb-20"
    >
      {/* Dossier Header */}
      <Card className="p-0 overflow-hidden border-white/5 relative bg-white/[0.02]">
        <div className="absolute top-0 left-0 w-full h-1.5 bg-gradient-to-r from-transparent via-primary/40 to-transparent" />
        <div className="p-8 md:p-12">
          <div className="flex flex-col md:flex-row gap-8 items-center md:items-start text-center md:text-left">
            <div className="relative group flex-shrink-0">
              <div className="w-32 h-32 md:w-40 md:h-40 rounded-lg overflow-hidden border-2 border-white/10 shadow-2xl relative">
                {avatarUrl ? (
                  <img
                    src={avatarUrl}
                    alt={profile.discord_name}
                    className="w-full h-full object-cover grayscale group-hover:grayscale-0 transition-all duration-500"
                  />
                ) : (
                  <div className="w-full h-full bg-white/5 flex items-center justify-center">
                    <User size={64} className="text-muted opacity-20" />
                  </div>
                )}
                {isOwnProfile && (
                  <button
                    type="button"
                    onClick={() => avatarInputRef.current?.click()}
                    disabled={uploadAvatar.isPending}
                    className="absolute inset-0 bg-black/60 flex flex-col items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity"
                  >
                    <Camera size={24} className="text-primary mb-1" />
                    <span className="text-[10px] font-bold uppercase tracking-widest text-white">Siegel ändern</span>
                  </button>
                )}
              </div>
              {isOwnProfile && (
                <input
                  ref={avatarInputRef}
                  type="file"
                  accept="image/jpeg,image/png,image/webp"
                  className="hidden"
                  onChange={handleAvatarChange}
                />
              )}
            </div>

            <div className="flex-1 space-y-4">
              <div className="space-y-1">
                <p className="text-[10px] font-bold text-primary uppercase tracking-[0.4em]">Profil-Dossier</p>
                {editName && isOwnProfile ? (
                  <div className="flex items-center justify-center md:justify-start gap-2">
                    <input
                      value={nameValue}
                      onChange={(e) => setNameValue(e.target.value)}
                      className="bg-white/5 border border-white/10 rounded-lg px-3 py-2 text-xl font-bold font-display uppercase tracking-tight text-foreground focus:outline-none focus:border-primary/50"
                      autoFocus
                    />
                    <Button variant="ghost" size="sm" onClick={() => setEditName(false)}><X size={14} /></Button>
                    <Button variant="primary" size="sm" onClick={handleNameSave} disabled={updateProfile.isPending}><Check size={14} /></Button>
                  </div>
                ) : (
                  <div className="flex items-center justify-center md:justify-start gap-3 group">
                    <h1 className="text-4xl md:text-5xl font-bold font-display text-foreground tracking-tight uppercase">
                      {(isOwnProfile && myProfile?.display_name) ? myProfile.display_name : profile.discord_name}
                    </h1>
                    {isOwnProfile && (
                      <button onClick={handleNameEdit} className="text-muted hover:text-primary transition-colors opacity-0 group-hover:opacity-100">
                        <Edit2 size={16} />
                      </button>
                    )}
                  </div>
                )}
              </div>

              <div className="flex flex-wrap justify-center md:justify-start gap-4">
                <div className="px-3 py-1 rounded-lg border border-primary/20 bg-primary/5 text-[10px] font-bold text-primary uppercase tracking-widest">
                  {profile.rank ?? 'Rekrut'}
                </div>
                <div className="px-3 py-1 rounded-lg border border-white/10 bg-white/5 text-[10px] font-bold text-muted uppercase tracking-widest">
                  Score: {profile.rank_score}
                </div>
              </div>

              {/* Bio section */}
              <div className="pt-4 border-t border-white/5 max-w-xl">
                {editBio && isOwnProfile ? (
                  <div className="space-y-3">
                    <textarea
                      value={bioValue}
                      onChange={(e) => setBioValue(e.target.value)}
                      className="w-full bg-white/5 border border-white/10 rounded-lg p-4 text-sm italic text-muted focus:outline-none focus:border-primary/50 min-h-[100px]"
                      placeholder="Deine Geschichte..."
                    />
                    <div className="flex gap-2">
                      <Button size="sm" onClick={handleBioSave} disabled={updateProfile.isPending}>Sichern</Button>
                      <Button variant="ghost" size="sm" onClick={() => setEditBio(false)}>Abbruch</Button>
                    </div>
                  </div>
                ) : (
                  <div className="group relative">
                    <p className="text-sm italic text-muted leading-relaxed">
                      {profile.bio ? `"${profile.bio}"` : '"Noch wurde keine Legende über diesen Spieler geschrieben."'}
                    </p>
                    {isOwnProfile && (
                      <button onClick={handleBioEdit} className="absolute -top-6 right-0 text-muted hover:text-primary opacity-0 group-hover:opacity-100 transition-all">
                        <Edit2 size={14} />
                      </button>
                    )}
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      </Card>

      {/* Stats Grid */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <Card className="text-center p-6 space-y-2 border-white/5">
          <Star size={20} className="mx-auto text-primary" />
          <p className="text-2xl font-bold text-foreground">{profile.total_points}</p>
          <p className="text-[10px] uppercase font-bold text-muted tracking-widest">Punkte</p>
        </Card>
        <Card className="text-center p-6 space-y-2 border-white/5">
          <Trophy size={20} className="mx-auto text-yellow-400" />
          <p className="text-2xl font-bold text-foreground">{profile.tournaments_played}</p>
          <p className="text-[10px] uppercase font-bold text-muted tracking-widest">Turniere</p>
        </Card>
        <Card className="text-center p-6 space-y-2 border-white/5">
          <Swords size={20} className="mx-auto text-blue-400" />
          <p className="text-2xl font-bold text-foreground">{profile.matches_played}</p>
          <p className="text-[10px] uppercase font-bold text-muted tracking-widest">Einsätze</p>
        </Card>
        <Card className="text-center p-6 space-y-2 border-white/5">
          <Target size={20} className="mx-auto text-green-400" />
          <p className="text-2xl font-bold text-foreground">{profile.matches_won}</p>
          <p className="text-[10px] uppercase font-bold text-muted tracking-widest">Siege</p>
        </Card>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
        {/* Tournament History */}
        <div className="space-y-6">
          <h2 className="text-xl font-bold font-display uppercase tracking-widest text-foreground flex items-center gap-3">
            <ScrollText size={20} className="text-primary" />
            Chroniken
          </h2>
          <div className="space-y-4">
            {profile.tournament_history && profile.tournament_history.length > 0 ? (
              profile.tournament_history.map((entry, idx) => (
                <Card key={idx} className="p-4 border-white/5 bg-white/[0.01] hover:bg-white/[0.03] transition-all">
                  <div className="flex items-center justify-between">
                    <div>
                      <h3 className="font-bold text-sm uppercase tracking-tight text-foreground">{entry.tournament_name}</h3>
                      {entry.team_name && <p className="text-[10px] text-muted uppercase tracking-widest mb-1">{entry.team_name}</p>}
                      <p className="text-[10px] text-muted italic">{placementLabel(entry.placement)}</p>
                    </div>
                    <ArrowUpRight size={14} className="text-muted" />
                  </div>
                </Card>
              ))
            ) : (
              <p className="text-sm italic text-muted">Noch keine Schlachten geschlagen.</p>
            )}
          </div>
        </div>

        {/* Settings / Actions for Own Profile */}
        {isOwnProfile && (
          <div className="space-y-6">
            <h2 className="text-xl font-bold font-display uppercase tracking-widest text-foreground flex items-center gap-3">
              <Settings size={20} className="text-primary" />
              Präferenzen
            </h2>
            <Card className="p-6 space-y-6 border-white/5">
               <div className="space-y-4">
                  <div className="flex items-center justify-between gap-4 border-b border-white/5 pb-4">
                    <div>
                      <p className="text-sm font-bold text-foreground uppercase tracking-wide">Auto-Rekrutierung</p>
                      <p className="text-[10px] text-muted italic">Einladungen automatisch annehmen.</p>
                    </div>
                    <input
                      type="checkbox"
                      checked={myProfile?.invite_auto_accept}
                      onChange={(e) => handleSettingsSave({ invite_auto_accept: e.target.checked })}
                      className="accent-primary w-4 h-4"
                    />
                  </div>
                  <div className="flex items-center justify-between gap-4">
                    <div>
                      <p className="text-sm font-bold text-foreground uppercase tracking-wide">Discord Bot-Botschaften</p>
                      <p className="text-[10px] text-muted italic">Benachrichtigungen via DM erhalten.</p>
                    </div>
                    <input
                      type="checkbox"
                      checked={myProfile?.notify_discord_dm}
                      onChange={(e) => handleSettingsSave({ notify_discord_dm: e.target.checked })}
                      className="accent-primary w-4 h-4"
                    />
                  </div>
               </div>

               {settingsSaved && (
                 <p className="text-[10px] text-green-400 font-bold uppercase tracking-widest animate-pulse">
                   Direktive gespeichert.
                 </p>
               )}

               <div className="pt-6 border-t border-white/5 space-y-4">
                 <button
                   onClick={handleRevokeConsent}
                   className="text-[10px] font-bold text-red-400 uppercase tracking-widest hover:text-red-300 transition-colors"
                 >
                   Einwilligung widerrufen
                 </button>
               </div>
            </Card>

            {consentMessage && (
              <p className="text-xs italic text-amber-500 bg-amber-500/10 p-3 border border-amber-500/20 rounded-lg">
                {consentMessage}
              </p>
            )}

            <DmSubscriptionPanel />
          </div>
        )}
      </div>
    </motion.div>
  )
}
