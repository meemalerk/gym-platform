import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { useEffect, useState } from 'react';
import { ActivityIndicator } from 'react-native';

import { ApiError } from '@/api/client';
import {
  getMyProfiles,
  renameMe,
  updateAthleteProfile,
  updateTrainerProfile,
} from '@/api/gym';
import { can, useSession } from '@/session/store';
import { Button, Centered, ErrorBanner, Field, Muted, Screen, Section } from '@/ui/components';
import { DateField, NumberField } from '@/ui/fields';
import { useTokens } from '@/ui/theme-context';

/**
 * One form for the whole identity (ADR-0014): the account's name, the athlete
 * profile, and — for anyone who coaches anywhere — the trainer profile. Saved
 * as full replaces: what you cleared stays cleared.
 */
export default function EditProfile() {
  const t = useTokens();
  const router = useRouter();
  const queryClient = useQueryClient();
  const user = useSession((s) => s.user);
  const membership = useSession((s) => s.membership);

  // The trainer section follows the ACCOUNT, not a gym-scoped view: you edit
  // your coaching bio even on a screen that otherwise reads as member-only.
  const coachesAnywhere = membership != null && can.coach(membership.capacities);

  const profiles = useQuery({ queryKey: ['my-profiles'], queryFn: getMyProfiles });

  const [displayName, setDisplayName] = useState(user?.displayName ?? '');
  const [goals, setGoals] = useState('');
  const [heightCm, setHeightCm] = useState('');
  const [trainingAge, setTrainingAge] = useState('');
  const [limitations, setLimitations] = useState('');
  const [dob, setDob] = useState('');
  const [headline, setHeadline] = useState('');
  const [bio, setBio] = useState('');
  const [certifications, setCertifications] = useState('');
  const [specialties, setSpecialties] = useState('');
  const [error, setError] = useState<string | null>(null);

  // Prefill once the server answers; a re-fetch must not stomp mid-edit state,
  // hence keying on data identity rather than re-running per render.
  useEffect(() => {
    const a = profiles.data?.athlete;
    const t = profiles.data?.trainer;
    if (a) {
      setGoals(a.goals ?? '');
      setHeightCm(a.height_cm != null ? `${a.height_cm}` : '');
      setTrainingAge(a.training_age_months != null ? `${a.training_age_months}` : '');
      setLimitations(a.limitations ?? '');
      setDob(a.date_of_birth ?? '');
    }
    if (t) {
      setHeadline(t.headline ?? '');
      setBio(t.bio ?? '');
      setCertifications(t.certifications.join(', '));
      setSpecialties(t.specialties.join(', '));
    }
  }, [profiles.data]);

  const save = useMutation({
    mutationFn: async () => {
      if (displayName.trim() !== user?.displayName) {
        await renameMe(displayName);
      }
      await updateAthleteProfile({
        goals: goals.trim() === '' ? null : goals,
        height_cm: heightCm.trim() === '' ? null : Number.parseInt(heightCm, 10),
        training_age_months: trainingAge.trim() === '' ? null : Number.parseInt(trainingAge, 10),
        limitations: limitations.trim() === '' ? null : limitations,
        date_of_birth: dob.trim() === '' ? null : dob.trim(),
      });
      if (coachesAnywhere) {
        const split = (raw: string) =>
          raw
            .split(',')
            .map((s) => s.trim())
            .filter((s) => s.length > 0);
        await updateTrainerProfile({
          headline: headline.trim() === '' ? null : headline,
          bio: bio.trim() === '' ? null : bio,
          certifications: split(certifications),
          specialties: split(specialties),
        });
      }
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['my-profiles'] });
      router.back();
    },
    onError: (e: Error) => {
      // The server's validation message names the field and the bound — worth
      // showing verbatim rather than flattening into "something went wrong".
      setError(
        e instanceof ApiError && e.code === 'request.invalid'
          ? e.message
          : 'Could not save your profile. Please try again.',
      );
    },
  });

  const ageOk = trainingAge.trim() === '' || /^\d+$/.test(trainingAge);
  const heightOk = heightCm.trim() === '' || /^\d+$/.test(heightCm);
  const dobOk = dob.trim() === '' || /^\d{4}-\d{2}-\d{2}$/.test(dob.trim());
  const ready =
    displayName.trim().length > 0 && ageOk && dobOk && heightOk && !save.isPending;

  if (profiles.isLoading) {
    return (
      <Centered>
        <ActivityIndicator color={t.color.accent} />
      </Centered>
    );
  }

  return (
    <Screen scroll edges={['bottom']}>
      {error ? <ErrorBanner message={error} /> : null}
      {profiles.isError ? <ErrorBanner message="Could not load your current profile." /> : null}

      <Field
        label="Display name"
        value={displayName}
        onChangeText={setDisplayName}
        autoCapitalize="words"
        error={displayName.trim().length === 0 ? 'A name is required.' : undefined}
      />

      <Section label="About your training" />
      <Muted>Your coaches see this — it belongs to you, not the gym.</Muted>
      <Field
        label="Goals"
        value={goals}
        onChangeText={setGoals}
        multiline
        placeholder="What are you working towards?"
      />
      <NumberField
        label="Height"
        value={heightCm}
        onChange={setHeightCm}
        unit="cm"
        decimals={0}
        placeholder="178 — used for BMI"
      />
      <NumberField
        label="Training age"
        value={trainingAge}
        onChange={setTrainingAge}
        unit="months"
        decimals={0}
        placeholder="18"
      />
      <Field
        label="Limitations & injuries"
        value={limitations}
        onChangeText={setLimitations}
        multiline
        placeholder="Anything a coach must know — e.g. left knee: no deep lunges."
      />
      <DateField
        label="Date of birth"
        value={dob}
        onChange={setDob}
        // The domain floor is 13; the picker simply cannot offer younger.
        maximumDate={new Date(new Date().getFullYear() - 13, new Date().getMonth(), new Date().getDate())}
        minimumDate={new Date(1906, 0, 1)}
        placeholder="Pick your birthday"
      />

      {coachesAnywhere ? (
        <>
          <Section label="Your coaching profile" />
          <Muted>Shown wherever you coach — specialties feed recommendations.</Muted>
          <Field
            label="Headline"
            value={headline}
            onChangeText={setHeadline}
            placeholder="e.g. Strength coach — beginners a specialty"
          />
          <Field
            label="Bio"
            value={bio}
            onChangeText={setBio}
            multiline
            placeholder="A few sentences about how you coach."
          />
          <Field
            label="Certifications"
            value={certifications}
            onChangeText={setCertifications}
            placeholder="Comma-separated, e.g. CSCS, PN L1"
            autoCapitalize="characters"
          />
          <Field
            label="Specialties"
            value={specialties}
            onChangeText={setSpecialties}
            placeholder="Comma-separated, e.g. Powerlifting, Return from injury"
          />
        </>
      ) : null}

      <Button
        label={save.isPending ? 'Saving…' : 'Save profile'}
        disabled={!ready}
        onPress={() => {
          setError(null);
          save.mutate();
        }}
      />
    </Screen>
  );
}
