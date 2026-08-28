/**
 * A trend line — the shape of a number over time, nothing more.
 *
 * A round-capped 2.5px stroke over a soft accent wash, with the latest point
 * as a filled dot inside a halo of the card's own ground. That halo is the
 * detail that makes it read: without it, the newest marker merges into the
 * line whenever the series ends on a flat run, and "where am I now" is the one
 * value people look for first.
 *
 * Earlier points are quiet ticks rather than markers. Marking all of them
 * turns a trend into a scatter plot, which is a different claim about the data
 * than the one this chart is making.
 *
 * Coordinates are computed in real pixels from `onLayout` rather than leaning
 * on `preserveAspectRatio="none"`, which stretches the stroke along with the
 * geometry and makes the line thicker at one end than the other.
 */

import { useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import Svg, { Circle, Path, Polyline } from 'react-native-svg';

import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

export function TrendChart({
  values,
  height = 130,
  startLabel,
  endLabel,
  accessibilityLabel,
}: {
  /** Oldest first. Fewer than two points renders nothing — a line needs two. */
  values: number[];
  height?: number;
  /** Captions under the ends, e.g. "28 Jun · 76.2". */
  startLabel?: string;
  endLabel?: string;
  accessibilityLabel?: string;
}) {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const [width, setWidth] = useState(0);

  if (values.length < 2) return null;

  const min = Math.min(...values);
  const max = Math.max(...values);
  // A flat series would divide by zero; draw it down the middle instead.
  const span = max - min || 1;
  const pad = 8;
  const usableH = height - pad * 2;

  const x = (i: number) => (width * i) / (values.length - 1);
  const y = (v: number) => pad + (1 - (v - min) / span) * usableH;

  const points = values.map((v, i) => `${x(i)},${y(v)}`).join(' ');
  const area = `M${points.split(' ').join(' L')} L${width},${height} L0,${height} Z`;

  const lastIndex = values.length - 1;

  return (
    <View
      style={s.wrap}
      onLayout={(e) => setWidth(e.nativeEvent.layout.width)}
      accessibilityLabel={accessibilityLabel}
    >
      <View style={{ height }}>
        {width > 0 ? (
          <Svg width={width} height={height}>
            <Path d={area} fill={t.color.accent} fillOpacity={0.09} />
            <Polyline
              points={points}
              fill="none"
              stroke={t.color.accent}
              strokeWidth={2.5}
              strokeLinecap="round"
              strokeLinejoin="round"
            />
            {values.slice(1, -1).map((v, i) => (
              <Circle key={i} cx={x(i + 1)} cy={y(v)} r={2} fill={t.color.track} />
            ))}
            {/* Halo first, then the dot: two circles, so the marker keeps its
                own edge wherever the line happens to run underneath it. */}
            <Circle
              cx={x(lastIndex)}
              cy={y(values[lastIndex]!)}
              r={6.5}
              fill={t.color.surface2}
            />
            <Circle
              cx={x(lastIndex)}
              cy={y(values[lastIndex]!)}
              r={4.5}
              fill={t.color.accent}
            />
          </Svg>
        ) : null}
      </View>

      {startLabel || endLabel ? (
        <View style={s.ends}>
          <Text style={s.endText}>{startLabel ?? ''}</Text>
          <Text style={s.endText}>{endLabel ?? ''}</Text>
        </View>
      ) : null}
    </View>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    wrap: { gap: t.space.sm },
    ends: {
      borderTopColor: t.color.line,
      borderTopWidth: StyleSheet.hairlineWidth,
      flexDirection: 'row',
      justifyContent: 'space-between',
      paddingTop: 8,
    },
    endText: {
      color: t.color.mut,
      fontFamily: fonts.medium,
      fontSize: t.font.xs,
      fontVariant: ['tabular-nums'],
    },
  });
