#!/usr/bin/env python3
"""Host regression for caller flags at the POSIX std build boundary."""
import sys
import unittest

sys.dont_write_bytecode = True
from build_posix_std import inherited_rustflags


class CallerFlagsTests(unittest.TestCase):
    def test_plain_flags_preserve_quoted_cfg_and_paths(self):
        self.assertEqual(
            inherited_rustflags({'RUSTFLAGS': '--cfg \'app="two words"\' -L "/tmp/library path" -Cpanic=unwind'}),
            ['--cfg', 'app="two words"', '-L', '/tmp/library path', '-Cpanic=unwind'],
        )

    def test_encoded_flags_take_precedence_even_when_empty(self):
        env = {'RUSTFLAGS': '--cfg=wrong',
               'CARGO_ENCODED_RUSTFLAGS': '--cfg\x1fapp="two words"\x1f-L\x1f/tmp/library path'}
        self.assertEqual(inherited_rustflags(env), ['--cfg', 'app="two words"', '-L', '/tmp/library path'])
        env['CARGO_ENCODED_RUSTFLAGS'] = '-L\x1f\x1f--cfg=app'
        self.assertEqual(inherited_rustflags(env), ['-L', '', '--cfg=app'])
        env['CARGO_ENCODED_RUSTFLAGS'] = ''
        self.assertEqual(inherited_rustflags(env), [])
        self.assertEqual(inherited_rustflags({}), [])

    def test_malformed_quoted_flags_fail_instead_of_silently_losing_options(self):
        with self.assertRaises(ValueError):
            inherited_rustflags({'RUSTFLAGS': '--cfg "unterminated'})


if __name__ == '__main__':
    unittest.main()
