import unittest

import subtitles


class Subtitles(unittest.TestCase):
    def test_wrap_balances_two_lines_and_rejects_three(self):
        text = "We'll build a small table from flat shapes, using five basic tools."
        lines = subtitles.wrap(text)
        self.assertEqual(" ".join(lines), text)
        self.assertEqual(len(lines), 2)
        self.assertLess(abs(len(lines[0]) - len(lines[1])), 12)
        self.assertEqual(subtitles.wrap("Short cue."), ["Short cue."])
        with self.assertRaises(ValueError):
            subtitles.wrap("word " * 40)

    def test_reading_time_has_a_floor_and_grows_with_length(self):
        self.assertEqual(subtitles.reading_seconds("Hi"), subtitles.MIN_CUE_SECONDS + 0.4)
        self.assertGreater(subtitles.reading_seconds("x" * 100), 7)

    def test_formats_carry_the_same_times(self):
        cues = [dict(start=3661.5, end=3662.25, text="One")]
        self.assertIn("01:01:01,500 --> 01:01:02,250\nOne", subtitles.srt(cues))
        self.assertTrue(subtitles.vtt(cues).startswith("WEBVTT\n"))
        self.assertIn("01:01:01.500 --> 01:01:02.250\nOne", subtitles.vtt(cues))

    def test_overlay_marks_clicks_and_keys_but_never_burns_subtitles(self):
        view = dict(x=232, y=159, width=1270, height=895)
        titles = [dict(start=0, end=3, title="Use {braces}", subtitle=None)]
        clicks = [dict(t=2, x=10, y=20)]
        keys = [dict(start=2, end=3, text="T")]
        snaps = [dict(start=1, end=2, x=100, y=200, text="Axis Guide")]
        script = subtitles.overlay(clicks, titles, view, keys, snaps)
        self.assertIn("\\pos(126,226)", script)
        self.assertIn("}Axis Guide", script)
        self.assertIn("Use \\{braces\\}", script)
        self.assertIn("\\pos(10,20)", script)
        self.assertIn("}T", script)
        self.assertNotIn(",Cue,", script)

    def test_spoken_form_and_cache_key_follow_what_is_said(self):
        self.assertEqual(subtitles.spoken("Push/Pull 5 cm"), "push pull 5 centimetres")
        self.assertEqual(subtitles.voice_key("Push/Pull"), subtitles.voice_key("push pull"))
        self.assertNotEqual(subtitles.voice_key("Nice"), subtitles.voice_key("Niiiice"))

    def test_audit_reports_fast_and_overlapping_cues(self):
        good = dict(start=0, end=4, text="Fine")
        self.assertEqual(subtitles.audit([good]), [])
        fast = dict(start=0, end=1, text="x" * 56)
        self.assertIn("too fast", subtitles.audit([fast])[0])
        overlap = [good, dict(start=3, end=7, text="Late")]
        self.assertIn("overlaps", subtitles.audit(overlap)[0])


if __name__ == "__main__":
    unittest.main()
