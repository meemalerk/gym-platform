/**
 * Purpose-built inputs. A date is picked, not typed; a number under a gym thumb
 * is stepped, not hunted for on a keyboard. Both wear the same recessed trough
 * as `Field`, so a form reads as one family rather than as three controls that
 * arrived from different screens.
 */

import Feather from '@expo/vector-icons/Feather';
import DateTimePicker from '@react-native-community/datetimepicker';
import { useState } from 'react';
import { Platform, StyleSheet, Text, TextInput, View } from 'react-native';

import { Touchable } from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/** YYYY-MM-DD ⇄ Date, in the device's local calendar — never UTC parsing,
 * which shifts "1994-03-12" a day west of Greenwich. */
const toDate = (iso: string): Date | null => {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso.trim());
  if (!m) return null;
  return new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
};

const toIso = (d: Date): string =>
  `${d.getFullYear()}-${`${d.getMonth() + 1}`.padStart(2, '0')}-${`${d.getDate()}`.padStart(2, '0')}`;

/**
 * A tap-to-pick date. Renders the native calendar (Android: dialog; iOS:
 * inline spinner below the row). The value travels as YYYY-MM-DD.
 */
export function DateField({
  label,
  value,
  onChange,
  minimumDate,
  maximumDate,
  placeholder = 'Pick a date',
}: {
  label: string;
  value: string;
  onChange: (iso: string) => void;
  minimumDate?: Date;
  maximumDate?: Date;
  placeholder?: string;
}) {
  const t = useTokens();
  const styles = useStyles(styleFactory);
  const [open, setOpen] = useState(false);
  const selected = toDate(value);

  return (
    <View style={styles.group}>
      <Text style={styles.label}>{label}</Text>
      <Touchable
        onPress={() => setOpen((v) => !v)}
        accessibilityRole="button"
        accessibilityLabel={`${label}: ${selected ? value : 'not set'}. Opens a date picker.`}
        style={styles.dateRow}
      >
        <Feather name="calendar" size={16} color={t.color.mut} />
        <Text style={selected ? styles.dateValue : styles.datePlaceholder}>
          {selected
            ? selected.toLocaleDateString(undefined, {
                day: 'numeric',
                month: 'long',
                year: 'numeric',
              })
            : placeholder}
        </Text>
        {selected ? (
          <Touchable
            onPress={() => onChange('')}
            accessibilityRole="button"
            accessibilityLabel={`Clear ${label}`}
            style={styles.clear}
          >
            <Feather name="x" size={14} color={t.color.faint} />
          </Touchable>
        ) : null}
      </Touchable>

      {open && Platform.OS === 'web' ? (
        // The native picker throws on web. Web is a dev/verification surface
        // only (see secure-storage.ts), so a typed ISO date is honest enough.
        <TextInput
          value={value}
          onChangeText={onChange}
          placeholder="YYYY-MM-DD"
          placeholderTextColor={t.color.faint}
          style={styles.webDateInput}
          accessibilityLabel={`${label}, typed as year dash month dash day`}
        />
      ) : open ? (
        <DateTimePicker
          value={selected ?? maximumDate ?? new Date()}
          mode="date"
          display={Platform.OS === 'ios' ? 'spinner' : 'default'}
          minimumDate={minimumDate}
          maximumDate={maximumDate}
          onChange={(event, date) => {
            // Android fires once and closes; dismissing must not write a value.
            if (Platform.OS === 'android') setOpen(false);
            if (event.type !== 'dismissed' && date) onChange(toIso(date));
          }}
        />
      ) : null}
    </View>
  );
}

/**
 * A number with a unit and, when `step` is given, thumb-sized − / + buttons.
 * Typing stays possible — steppers are for mid-set hands, not a replacement.
 */
export function NumberField({
  label,
  value,
  onChange,
  unit,
  step,
  min = 0,
  max = 10_000,
  decimals = 1,
  width,
  placeholder,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  unit?: string;
  /** Enables the − / + buttons, stepping by this much. */
  step?: number;
  min?: number;
  max?: number;
  decimals?: number;
  width?: number;
  placeholder?: string;
}) {
  const t = useTokens();
  const styles = useStyles(styleFactory);

  const bump = (direction: 1 | -1) => {
    const current = Number.parseFloat(value);
    const base = Number.isFinite(current) ? current : min;
    const next = Math.min(max, Math.max(min, base + direction * (step ?? 1)));
    // Trim trailing zeros: "72.5" not "72.50", "5" not "5.0".
    onChange(`${Number(next.toFixed(decimals))}`);
  };

  return (
    <View style={[styles.group, width != null && { width }]}>
      <Text style={styles.label}>{label}</Text>
      <View style={styles.numberRow}>
        {step != null ? (
          <Touchable
            onPress={() => bump(-1)}
            accessibilityRole="button"
            accessibilityLabel={`Decrease ${label}`}
            style={styles.stepper}
          >
            <Feather name="minus" size={16} color={t.color.mut} />
          </Touchable>
        ) : null}
        <View style={styles.inputWrap}>
          <TextInput
            value={value}
            onChangeText={onChange}
            keyboardType="decimal-pad"
            placeholder={placeholder}
            placeholderTextColor={t.color.faint}
            style={styles.input}
            accessibilityLabel={unit ? `${label} in ${unit}` : label}
          />
          {unit ? <Text style={styles.unit}>{unit}</Text> : null}
        </View>
        {step != null ? (
          <Touchable
            onPress={() => bump(1)}
            accessibilityRole="button"
            accessibilityLabel={`Increase ${label}`}
            style={styles.stepper}
          >
            <Feather name="plus" size={16} color={t.color.accent} />
          </Touchable>
        ) : null}
      </View>
    </View>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    group: { gap: 7 },
    label: {
      color: t.color.mut2,
      fontFamily: fonts.semibold,
      fontSize: t.font.sm,
    },

    // Same recessed trough as `Field`: a form should read as one family, and
    // a date that looked like a button while a name looked like a well would
    // make the picker feel like it belonged to a different screen.
    dateRow: {
      alignItems: 'center',
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.md,
      flexDirection: 'row',
      gap: t.space.md,
      minHeight: 50,
      paddingHorizontal: 14,
    },
    dateValue: {
      color: t.color.ink,
      flex: 1,
      fontFamily: fonts.medium,
      fontSize: t.font.md,
    },
    datePlaceholder: {
      color: t.color.faint,
      flex: 1,
      fontFamily: fonts.regular,
      fontSize: t.font.md,
    },
    clear: {
      alignItems: 'center',
      borderRadius: t.radius.pill,
      justifyContent: 'center',
      minHeight: 32,
      minWidth: 32,
    },
    webDateInput: {
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.md,
      color: t.color.ink,
      fontFamily: fonts.medium,
      fontSize: t.font.md,
      minHeight: 50,
      paddingHorizontal: 14,
    },

    numberRow: { alignItems: 'center', flexDirection: 'row', gap: t.space.sm },
    stepper: {
      alignItems: 'center',
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.md,
      height: 50,
      justifyContent: 'center',
      width: 50,
    },
    inputWrap: {
      alignItems: 'center',
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.md,
      flex: 1,
      flexDirection: 'row',
      minHeight: 50,
      paddingHorizontal: 14,
    },
    input: {
      color: t.color.ink,
      flex: 1,
      // The value is the point of the control, so it gets the display face and
      // tabular figures — a bodyweight that reflows as you type it reads as
      // instability in the number itself.
      fontFamily: fonts.display,
      fontSize: t.font.xl,
      fontVariant: ['tabular-nums'],
      paddingVertical: 0,
    },
    unit: {
      color: t.color.mut,
      fontFamily: fonts.bold,
      fontSize: t.font.sm,
    },
  });
