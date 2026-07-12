"""M1 transport compatibility and engine-termination contracts."""

from __future__ import annotations

import io
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from client import (
    PROTOCOL_VERSION,
    ProtocolCompatibilityError,
    ShipsimSession,
    TransportError,
)


class _FakeStdin(io.StringIO):
    def __init__(self, failure: OSError | None = None, flush_failure: OSError | None = None) -> None:
        super().__init__()
        self.failure = failure
        self.flush_failure = flush_failure

    def write(self, text: str) -> int:
        if self.failure is not None:
            raise self.failure
        return super().write(text)

    def flush(self) -> None:
        if self.flush_failure is not None:
            raise self.flush_failure
        super().flush()


class _FakeProc:
    def __init__(
        self,
        inbound: str,
        *,
        stdin: _FakeStdin | None = None,
        read_failure: OSError | None = None,
    ) -> None:
        self.stdin = stdin or _FakeStdin()
        self.stdout = io.StringIO(inbound)
        if read_failure is not None:
            self.stdout.readline = lambda: (_ for _ in ()).throw(read_failure)  # type: ignore[method-assign]

    def poll(self) -> int:
        return 17

    def wait(self, timeout: int) -> int:
        return 17

    def kill(self) -> None:
        pass


def _session(inbound: str, **proc_args: object) -> ShipsimSession:
    session = object.__new__(ShipsimSession)
    session._proc = _FakeProc(inbound, **proc_args)  # type: ignore[arg-type]
    session.snapshot = None
    session.last_error = None
    session.orders = []
    session.stderr_log = Path("/tmp/shipsim-test-stderr.log")
    session.orders_log = Path("/tmp/shipsim-test-orders.jsonl")
    session._protocol_checked = False
    return session


class ProtocolCompatibilityTests(unittest.TestCase):
    def test_first_snapshot_accepts_supported_version(self) -> None:
        session = _session('{"type":"snapshot","protocol_version":2}\n')
        self.assertEqual(session._read_message()["protocol_version"], PROTOCOL_VERSION)

    def test_first_error_is_also_version_checked(self) -> None:
        session = _session('{"type":"error","protocol_version":1}\n')
        with self.assertRaisesRegex(
            ProtocolCompatibilityError, r"got 1.*supported 2"
        ):
            session._read_message()

    def test_missing_noninteger_and_unsupported_versions_are_fatal(self) -> None:
        cases = [
            ('{"type":"snapshot"}\n', "missing"),
            ('{"protocol_version":"1"}\n', "'1'"),
            ('{"protocol_version":9}\n', "9"),
        ]
        for inbound, got in cases:
            with self.subTest(got=got):
                with self.assertRaises(ProtocolCompatibilityError) as caught:
                    _session(inbound)._read_message()
                self.assertIn(f"got {got}", str(caught.exception))
                self.assertIn("supported 2", str(caught.exception))

    def test_version_is_checked_exactly_once(self) -> None:
        session = _session(
            '{"type":"snapshot","protocol_version":2}\n'
            '{"type":"snapshot","protocol_version":999}\n'
        )
        session._read_message()
        self.assertEqual(session._read_message()["protocol_version"], 999)


class TransportFailureTests(unittest.TestCase):
    def test_malformed_and_nonobject_json_are_typed_transport_failures(self) -> None:
        for inbound in ("not json\n", "[]\n", '"hello"\n'):
            with self.subTest(inbound=inbound):
                with self.assertRaisesRegex(TransportError, "stderr"):
                    _session(inbound)._read_message()

    def test_stdout_read_oserror_is_typed(self) -> None:
        session = _session("", read_failure=OSError("read exploded"))
        with self.assertRaisesRegex(TransportError, "read exploded"):
            session._read_message()

    def test_startup_read_oserror_is_typed(self) -> None:
        session = ShipsimSession("scenario.toml", repo=Path("/"), bin_path=Path("/bin/true"))
        with patch("client.subprocess.Popen", return_value=_FakeProc("", read_failure=OSError("startup read"))):
            with self.assertRaisesRegex(TransportError, "startup read"):
                session.start()

    def test_startup_eof_is_typed_and_names_diagnostic_log(self) -> None:
        session = ShipsimSession("scenario.toml", repo=Path("/"), bin_path=Path("/bin/true"))
        with patch("client.subprocess.Popen", return_value=_FakeProc("")):
            with self.assertRaises(TransportError) as caught:
                session.start()
        self.assertIn(str(session.stderr_log), str(caught.exception))

    def test_send_eof_is_typed_and_preserves_wire_payload(self) -> None:
        session = _session("")
        with tempfile.TemporaryDirectory() as tmp:
            session.orders_log = Path(tmp) / "orders.jsonl"
            with self.assertRaisesRegex(TransportError, r"exit=17"):
                session.send_order({"type": "pass", "ship": 4})
            self.assertEqual(
                session._proc.stdin.getvalue(),
                '{"type":"pass","ship":4,"protocol_version":2}\n',
            )

    def test_write_failure_is_typed_and_names_diagnostic_log(self) -> None:
        session = _session("", stdin=_FakeStdin(BrokenPipeError("pipe closed")))
        with tempfile.TemporaryDirectory() as tmp:
            session.orders_log = Path(tmp) / "orders.jsonl"
            with self.assertRaises(TransportError) as caught:
                session.send_order({"type": "pass", "ship": 4})
        self.assertIn("pipe closed", str(caught.exception))
        self.assertIn(str(session.stderr_log), str(caught.exception))

    def test_flush_and_post_order_read_failures_are_typed(self) -> None:
        cases = [
            (_session("", stdin=_FakeStdin(flush_failure=BrokenPipeError("flush pipe"))), "flush pipe"),
            (_session("", read_failure=OSError("post-order read")), "post-order read"),
        ]
        for session, message in cases:
            with self.subTest(message=message), tempfile.TemporaryDirectory() as tmp:
                session.orders_log = Path(tmp) / "orders.jsonl"
                with self.assertRaisesRegex(TransportError, message):
                    session.send_order({"type": "pass", "ship": 4})


if __name__ == "__main__":
    unittest.main()
