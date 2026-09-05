//! Playback queue exclusion predicate: `passes_filters` decides whether a
//! song is allowed into the queue under the configured `PlaybackFilters`.

use crate::config::PlaybackFilters;
use crate::subsonic::models::Child;

/// Whether `song` is allowed into the queue under `filters`. `true` = keep.
///
/// Each criterion is independent and only excludes when the song has the
/// relevant data: unrated songs, or songs with an unknown year/genre/artist,
/// are never excluded by a criterion they have no data for.
#[must_use]
pub fn passes_filters(song: &Child, filters: &PlaybackFilters) -> bool {
    if filters.min_rating > 0 {
        if let Some(rating) = song.user_rating {
            if rating <= filters.min_rating {
                return false;
            }
        }
    }

    if let Some(year) = song.year {
        if filters.year_min.is_some_and(|min| year < min) {
            return false;
        }
        if filters.year_max.is_some_and(|max| year > max) {
            return false;
        }
    }

    if let Some(duration) = song.duration.and_then(|d| u32::try_from(d).ok()) {
        if filters.duration_min_secs.is_some_and(|min| duration < min) {
            return false;
        }
        if filters.duration_max_secs.is_some_and(|max| duration > max) {
            return false;
        }
    }

    if let Some(genre) = song.genre.as_deref() {
        if filters
            .excluded_genres
            .iter()
            .any(|g| g.eq_ignore_ascii_case(genre))
        {
            return false;
        }
    }

    if let Some(artist) = song.artist.as_deref() {
        if filters
            .excluded_artists
            .iter()
            .any(|a| a.eq_ignore_ascii_case(artist))
        {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song() -> Child {
        Child {
            id: "s0".into(),
            title: "Test Song".into(),
            ..Default::default()
        }
    }

    #[test]
    fn default_filters_keep_everything() {
        assert!(passes_filters(&song(), &PlaybackFilters::default()));
    }

    #[test]
    fn min_rating_excludes_at_or_below_threshold() {
        let filters = PlaybackFilters {
            min_rating: 2,
            ..Default::default()
        };
        let mut s = song();
        s.user_rating = Some(1);
        assert!(
            !passes_filters(&s, &filters),
            "1-star excluded at threshold 2"
        );
        s.user_rating = Some(2);
        assert!(
            !passes_filters(&s, &filters),
            "2-star excluded at threshold 2"
        );
        s.user_rating = Some(3);
        assert!(
            passes_filters(&s, &filters),
            "3-star kept above threshold 2"
        );
    }

    #[test]
    fn min_rating_never_excludes_unrated_songs() {
        let filters = PlaybackFilters {
            min_rating: 5,
            ..Default::default()
        };
        assert!(
            passes_filters(&song(), &filters),
            "an unrated song has no rating to compare against"
        );
    }

    #[test]
    fn min_rating_zero_disables_the_filter() {
        let filters = PlaybackFilters::default();
        let mut s = song();
        s.user_rating = Some(1);
        assert!(passes_filters(&s, &filters), "min_rating=0 means off");
    }

    #[test]
    fn year_range_excludes_outside_bounds_keeps_inside() {
        let filters = PlaybackFilters {
            year_min: Some(1970),
            year_max: Some(1979),
            ..Default::default()
        };
        let mut s = song();
        s.year = Some(1965);
        assert!(!passes_filters(&s, &filters), "before year_min");
        s.year = Some(1985);
        assert!(!passes_filters(&s, &filters), "after year_max");
        s.year = Some(1975);
        assert!(passes_filters(&s, &filters), "inside the range");
    }

    #[test]
    fn year_filter_never_excludes_unknown_year() {
        let filters = PlaybackFilters {
            year_min: Some(2000),
            ..Default::default()
        };
        assert!(passes_filters(&song(), &filters), "no year to compare");
    }

    #[test]
    fn duration_range_excludes_outside_bounds_keeps_inside() {
        let filters = PlaybackFilters {
            duration_min_secs: Some(60),
            duration_max_secs: Some(300),
            ..Default::default()
        };
        let mut s = song();
        s.duration = Some(30);
        assert!(!passes_filters(&s, &filters), "shorter than duration_min");
        s.duration = Some(400);
        assert!(!passes_filters(&s, &filters), "longer than duration_max");
        s.duration = Some(180);
        assert!(passes_filters(&s, &filters), "inside the range");
    }

    #[test]
    fn genre_exclusion_is_case_insensitive() {
        let filters = PlaybackFilters {
            excluded_genres: vec!["Podcast".into()],
            ..Default::default()
        };
        let mut s = song();
        s.genre = Some("podcast".into());
        assert!(!passes_filters(&s, &filters));
        s.genre = Some("Rock".into());
        assert!(passes_filters(&s, &filters));
    }

    #[test]
    fn artist_exclusion_is_case_insensitive() {
        let filters = PlaybackFilters {
            excluded_artists: vec!["Nickelback".into()],
            ..Default::default()
        };
        let mut s = song();
        s.artist = Some("NICKELBACK".into());
        assert!(!passes_filters(&s, &filters));
        s.artist = Some("Someone Else".into());
        assert!(passes_filters(&s, &filters));
    }

    #[test]
    fn multiple_criteria_combine_with_and_semantics() {
        // A song passing every criterion but one must still be excluded.
        let filters = PlaybackFilters {
            min_rating: 2,
            excluded_genres: vec!["Podcast".into()],
            ..Default::default()
        };
        let mut s = song();
        s.user_rating = Some(5); // passes rating
        s.genre = Some("Podcast".into()); // fails genre
        assert!(!passes_filters(&s, &filters));
    }
}
