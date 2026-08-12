package handler

import (
	"testing"

	"github.com/Wei-Shaw/sub2api/internal/pkg/timezone"
	"github.com/stretchr/testify/require"
)

// The public key-usage lookup accepts either a relative `days` window or an
// explicit daily_start_date / daily_end_date pair for the daily detail table.
func TestParseAPIKeyDailyUsageWindowFallsBackToRelativeDays(t *testing.T) {
	const tz = "Asia/Kolkata"

	start, end, ok := parseAPIKeyDailyUsageWindow("", "", 7, tz)
	require.True(t, ok)

	wantStart, wantEnd := apiKeyDailyUsageRange(7, tz)
	require.Equal(t, wantStart, start)
	require.Equal(t, wantEnd, end)
	require.Equal(t, 7*24.0, end.Sub(start).Hours())
}

func TestParseAPIKeyDailyUsageWindowUsesExplicitRange(t *testing.T) {
	// Negative UTC offset: the window must anchor to local midnight, not UTC.
	const tz = "America/New_York"

	start, end, ok := parseAPIKeyDailyUsageWindow("2026-07-01", "2026-07-03", 30, tz)
	require.True(t, ok)

	expectedStart, err := timezone.ParseInUserLocation("2006-01-02", "2026-07-01", tz)
	require.NoError(t, err)
	require.Equal(t, timezone.StartOfDayInUserLocation(expectedStart, tz), start)
	require.Equal(t, "2026-07-01", start.Format("2006-01-02"))
	// Half-open upper bound: the end day is fully included.
	require.Equal(t, "2026-07-04", end.Format("2006-01-02"))
	require.Equal(t, 3*24.0, end.Sub(start).Hours())
}

func TestParseAPIKeyDailyUsageWindowAcceptsMaxSpan(t *testing.T) {
	start, end, ok := parseAPIKeyDailyUsageWindow("2026-01-01", "2026-03-31", 30, "UTC")
	require.True(t, ok, "90 inclusive days must be accepted")
	require.Equal(t, float64(maxAPIKeyDailyUsageDays*24), end.Sub(start).Hours())
}

func TestParseAPIKeyDailyUsageWindowRejectsInvalidInput(t *testing.T) {
	cases := map[string][2]string{
		"missing end":     {"2026-07-01", ""},
		"missing start":   {"", "2026-07-03"},
		"inverted range":  {"2026-07-05", "2026-07-01"},
		"span over limit": {"2026-01-01", "2026-04-01"},
		"bad start":       {"07/01/2026", "2026-07-03"},
		"bad end":         {"2026-07-01", "not-a-date"},
	}

	for name, bounds := range cases {
		t.Run(name, func(t *testing.T) {
			_, _, ok := parseAPIKeyDailyUsageWindow(bounds[0], bounds[1], 30, "UTC")
			require.False(t, ok)
		})
	}
}
