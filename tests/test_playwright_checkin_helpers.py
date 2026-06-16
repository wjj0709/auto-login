import importlib.util
import json
import pathlib
import sys
import unittest
from unittest import mock


def load_module():
    script_path = pathlib.Path(__file__).resolve().parents[1] / "scripts" / "playwright_checkin.py"
    spec = importlib.util.spec_from_file_location("playwright_checkin", script_path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


playwright_checkin = load_module()


class PlaywrightCheckinHelpersTest(unittest.TestCase):
    def test_marks_execution_context_destroyed_as_transient(self):
        self.assertTrue(
            playwright_checkin.is_transient_page_error(
                "Page.evaluate: Execution context was destroyed, most likely because of a navigation"
            )
        )

    def test_ignores_non_navigation_errors(self):
        self.assertFalse(playwright_checkin.is_transient_page_error("Timeout 30000ms exceeded"))

    def test_normalizes_supported_sso_providers(self):
        self.assertEqual(playwright_checkin.normalize_sso_provider("GitHub"), "github")
        self.assertEqual(playwright_checkin.normalize_sso_provider("linux-do"), "linuxdo")
        self.assertEqual(playwright_checkin.normalize_sso_provider("linux.do"), "linuxdo")

    def test_rejects_unknown_sso_provider(self):
        with self.assertRaises(ValueError):
            playwright_checkin.normalize_sso_provider("google")

    def test_sso_button_selectors_cover_provider_names_and_urls(self):
        github_selectors = playwright_checkin.sso_button_selectors("github")
        linuxdo_selectors = playwright_checkin.sso_button_selectors("linuxdo")

        self.assertTrue(any("github" in item.lower() for item in github_selectors))
        self.assertTrue(any("linux" in item.lower() for item in linuxdo_selectors))
        self.assertTrue(any("href" in item for item in github_selectors))
        self.assertTrue(any("href" in item for item in linuxdo_selectors))

    def test_account_input_accepts_sso_fields(self):
        account = playwright_checkin.AccountInput(
            name="SSO",
            provider="anyrouter",
            domain="https://anyrouter.top",
            login_path="/login",
            sign_in_path="/api/user/sign_in",
            user_info_path="/api/user/self",
            api_user_key="new-api-user",
            api_user="148714",
            sso_provider="github",
            sso_username="alice",
            sso_password="secret",
        )

        self.assertEqual(account.sso_provider, "github")
        self.assertEqual(account.sso_username, "alice")
        self.assertEqual(account.sso_password, "secret")

    def test_manual_checkin_requires_successful_after_user_info(self):
        account = playwright_checkin.AccountInput(
            name="主账号",
            provider="anyrouter",
            domain="https://anyrouter.top",
            login_path="/login",
            sign_in_path="/api/user/sign_in",
            user_info_path="/api/user/self",
            api_user_key="new-api-user",
            api_user="148714",
        )

        success, error = playwright_checkin.determine_result_status(
            account,
            sign_ok=True,
            sign_err=None,
            after_ok=False,
            after_err="HTTP 401: New-Api-User 与登录用户不匹配",
        )

        self.assertFalse(success)
        self.assertIn("New-Api-User", error)

    def test_parse_sso_email_config_fills_defaults(self):
        config = playwright_checkin.parse_sso_email_config(
            {"imap_host": "imap.qq.com", "username": "a@qq.com", "password": "authcode"}
        )

        self.assertEqual(config["imap_host"], "imap.qq.com")
        self.assertEqual(config["imap_port"], 993)
        self.assertEqual(config["mailbox"], "INBOX")

    def test_parse_sso_email_config_requires_core_fields(self):
        self.assertIsNone(playwright_checkin.parse_sso_email_config(None))
        self.assertIsNone(playwright_checkin.parse_sso_email_config({"imap_host": "x"}))
        self.assertIsNone(
            playwright_checkin.parse_sso_email_config(
                {"imap_host": "x", "username": "u", "password": "   "}
            )
        )

    def test_extract_github_otp_prefers_labeled_code(self):
        body = "Hi 2026\nYour GitHub verification code is 481920.\nticket 999999"
        self.assertEqual(playwright_checkin.extract_github_otp(body), "481920")

    def test_extract_github_otp_falls_back_to_standalone_code(self):
        self.assertEqual(playwright_checkin.extract_github_otp("code: 135790 only"), "135790")
        self.assertIsNone(playwright_checkin.extract_github_otp("no digits here"))
        self.assertIsNone(playwright_checkin.extract_github_otp("1234567 too long"))

    def test_account_input_accepts_sso_email(self):
        account = playwright_checkin.AccountInput(
            name="主账号",
            provider="anyrouter",
            domain="https://anyrouter.top",
            login_path="/login",
            sign_in_path="/api/user/sign_in",
            user_info_path="/api/user/self",
            api_user_key="new-api-user",
            api_user="148714",
            sso_provider="github",
            sso_username="wjj0709",
            sso_password="secret",
            sso_email={"imap_host": "imap.qq.com", "username": "a@qq.com", "password": "code"},
        )

        self.assertEqual(account.sso_email["imap_host"], "imap.qq.com")

    def test_sanitize_recovers_surrogate_escaped_gbk_text(self):
        # 复现线上 bug：服务器返回的 GBK 字节经 surrogateescape 解码成孤立代理项，
        # 直接 json.dumps 会写出非法 UTF-8，导致 Rust 端整批解析失败。
        gbk_bytes = "用户名或密码错误".encode("gbk")
        mangled = gbk_bytes.decode("utf-8", "surrogateescape")

        cleaned = playwright_checkin.sanitize_for_json({"error": "login rejected: " + mangled})

        self.assertEqual(cleaned["error"], "login rejected: 用户名或密码错误")
        # 清洗后必须能编码为合法 UTF-8（Rust 才能解析）
        json.dumps(cleaned, ensure_ascii=False).encode("utf-8")

    def test_sanitize_guarantees_valid_utf8_for_unrecoverable_bytes(self):
        # 即便无法按已知编码还原，也必须产出合法 UTF-8，绝不残留孤立代理项
        mangled = b"\xff\xfe\x00bad".decode("utf-8", "surrogateescape")

        cleaned = playwright_checkin.sanitize_for_json(mangled)

        self.assertFalse(any("\ud800" <= ch <= "\udfff" for ch in cleaned))
        cleaned.encode("utf-8")  # 不抛 UnicodeEncodeError 即合法

    def test_sanitize_preserves_plain_text_and_structure(self):
        data = {"results": [{"name": "教育邮箱", "n": 1, "ok": True, "x": None}]}

        self.assertEqual(playwright_checkin.sanitize_for_json(data), data)

    def test_linuxdo_login_selectors_cover_username_password_submit(self):
        sel = playwright_checkin.linuxdo_login_selectors()
        self.assertTrue(sel["username"] and sel["password"] and sel["submit"])
        self.assertTrue(any("login-account-name" in s for s in sel["username"]))
        self.assertTrue(any("password" in s for s in sel["password"]))
        self.assertTrue(any("login-button" in s or "submit" in s for s in sel["submit"]))

    def test_detect_cloudflare_challenge_flags_known_markers(self):
        self.assertTrue(playwright_checkin.detect_cloudflare_challenge("Just a moment...", ""))
        self.assertTrue(
            playwright_checkin.detect_cloudflare_challenge("", "Checking your browser before accessing")
        )
        self.assertTrue(
            playwright_checkin.detect_cloudflare_challenge("Attention Required! | Cloudflare", "")
        )

    def test_detect_cloudflare_challenge_ignores_normal_login_page(self):
        self.assertFalse(
            playwright_checkin.detect_cloudflare_challenge("登录 - LINUX DO", "用户名 密码 登录")
        )

    def test_account_profile_dir_is_stable_and_per_account(self):
        import os
        import tempfile

        base = tempfile.mkdtemp()

        def acct(name, api_user):
            return playwright_checkin.AccountInput(
                name=name, provider="anyrouter", domain="https://x",
                login_path="/login", sign_in_path="/s", user_info_path="/u",
                api_user_key="new-api-user", api_user=api_user,
            )

        p1 = playwright_checkin.account_profile_dir(base, acct("教育邮箱", "151687"))
        p1_again = playwright_checkin.account_profile_dir(base, acct("教育邮箱", "151687"))
        p2 = playwright_checkin.account_profile_dir(base, acct("linuxdo_190030", "190030"))

        self.assertEqual(p1, p1_again)
        self.assertNotEqual(p1, p2)
        self.assertTrue(os.path.isdir(p1))
        self.assertTrue(os.path.isdir(p2))


class PlaywrightCheckinAsyncTest(unittest.IsolatedAsyncioTestCase):
    async def test_session_cookie_present_supports_custom_cookie_names(self):
        class FakeContext:
            async def cookies(self, _urls):
                return [{"name": "_t", "value": "abc"}]

        ctx = FakeContext()
        self.assertTrue(
            await playwright_checkin.session_cookie_present(
                ctx, "https://linux.do", ("_t", "_forum_session")
            )
        )
        self.assertFalse(
            await playwright_checkin.session_cookie_present(ctx, "https://linux.do", ("session",))
        )

    async def test_login_linuxdo_forum_fills_and_confirms_session(self):
        rec = []

        class FakeLocator:
            def __init__(self, selector):
                self._selector = selector

            @property
            def first(self):
                return self

            async def wait_for(self, state, timeout):
                pass

            async def fill(self, value, timeout=0):
                rec.append(("fill", self._selector, value))

            async def click(self, timeout=0, force=False):
                rec.append(("click", self._selector))

        class FakeContext:
            async def cookies(self, _urls):
                return [{"name": "_t", "value": "sess"}]

        class FakePage:
            def __init__(self, title):
                self._title = title
                self.context = FakeContext()
                self.url = "https://linux.do/login"

            async def goto(self, url, wait_until=None):
                rec.append(("goto", url))

            async def title(self):
                return self._title

            async def inner_text(self, selector, timeout=0):
                return ""

            def locator(self, selector):
                return FakeLocator(selector)

        account = playwright_checkin.AccountInput(
            name="L", provider="anyrouter", domain="https://anyrouter.top",
            login_path="/login", sign_in_path="/api/user/sign_in",
            user_info_path="/api/user/self", api_user_key="new-api-user", api_user="190030",
            sso_provider="linuxdo", sso_username="nianliu.wjj", sso_password="pw",
        )

        async def _noop(*args, **kwargs):
            return None

        with mock.patch.object(playwright_checkin, "wait_for_page_stability", _noop):
            await playwright_checkin.login_linuxdo_forum(FakePage("登录 - LINUX DO"), account, timeout_ms=5000)

        filled_values = [r[2] for r in rec if r[0] == "fill"]
        self.assertIn("nianliu.wjj", filled_values)
        self.assertIn("pw", filled_values)
        self.assertTrue(any(r[0] == "click" for r in rec))
        self.assertTrue(any(r == ("goto", "https://linux.do/login") for r in rec))

    async def test_login_linuxdo_forum_raises_on_cloudflare(self):
        rec = []

        class FakeLocator:
            def __init__(self, selector):
                self._selector = selector

            @property
            def first(self):
                return self

            async def wait_for(self, state, timeout):
                pass

            async def fill(self, value, timeout=0):
                rec.append(("fill", self._selector, value))

            async def click(self, timeout=0, force=False):
                rec.append(("click", self._selector))

        class FakeContext:
            async def cookies(self, _urls):
                return []

        class FakePage:
            def __init__(self, title):
                self._title = title
                self.context = FakeContext()
                self.url = "https://linux.do/login"

            async def goto(self, url, wait_until=None):
                rec.append(("goto", url))

            async def title(self):
                return self._title

            async def inner_text(self, selector, timeout=0):
                return ""

            def locator(self, selector):
                return FakeLocator(selector)

        account = playwright_checkin.AccountInput(
            name="L", provider="anyrouter", domain="https://anyrouter.top",
            login_path="/login", sign_in_path="/api/user/sign_in",
            user_info_path="/api/user/self", api_user_key="new-api-user", api_user="190030",
            sso_provider="linuxdo", sso_username="nianliu.wjj", sso_password="pw",
        )

        async def _noop(*args, **kwargs):
            return None

        with mock.patch.object(playwright_checkin, "wait_for_page_stability", _noop):
            with self.assertRaises(RuntimeError) as caught:
                await playwright_checkin.login_linuxdo_forum(FakePage("Just a moment..."), account, timeout_ms=5000)

        self.assertIn("Cloudflare", str(caught.exception))
        self.assertFalse(any(r[0] == "fill" for r in rec))

    async def test_click_first_available_uses_playwright_first_property(self):
        calls = []

        class FakeFirstLocator:
            async def wait_for(self, state, timeout):
                calls.append(("wait_for", state, timeout))

            async def click(self, timeout):
                calls.append(("click", timeout))

        class FakeLocator:
            @property
            def first(self):
                return FakeFirstLocator()

        class FakePage:
            def locator(self, selector):
                calls.append(("locator", selector))
                return FakeLocator()

        matched = await playwright_checkin.click_first_available(
            FakePage(),
            ["a[href*='github']"],
        )

        self.assertEqual("a[href*='github']", matched)
        self.assertEqual(("locator", "a[href*='github']"), calls[0])
        self.assertEqual(("click", 5000), calls[-1])

    async def test_click_first_available_force_clicks_when_overlay_intercepts(self):
        calls = []

        class FakeFirstLocator:
            async def wait_for(self, state, timeout):
                calls.append(("wait_for", state, timeout))

            async def click(self, timeout, force=False):
                calls.append(("click", timeout, force))
                if not force:
                    raise RuntimeError("subtree intercepts pointer events")

        class FakeLocator:
            @property
            def first(self):
                return FakeFirstLocator()

        class FakePage:
            def locator(self, selector):
                calls.append(("locator", selector))
                return FakeLocator()

        matched = await playwright_checkin.click_first_available(
            FakePage(),
            ["button[type='submit']"],
        )

        self.assertEqual("button[type='submit']", matched)
        self.assertIn(("click", 5000, True), calls)

    async def test_process_all_accounts_uses_isolated_context_per_account(self):
        accounts = [
            playwright_checkin.AccountInput(
                name="账号A",
                provider="anyrouter",
                domain="https://anyrouter.top",
                login_path="/login",
                sign_in_path="/api/user/sign_in",
                user_info_path="/api/user/self",
                api_user_key="new-api-user",
                api_user="111",
            ),
            playwright_checkin.AccountInput(
                name="账号B",
                provider="anyrouter",
                domain="https://anyrouter.top",
                login_path="/login",
                sign_in_path="/api/user/sign_in",
                user_info_path="/api/user/self",
                api_user_key="new-api-user",
                api_user="222",
            ),
        ]
        created_contexts = []
        processed = []

        class FakeContext:
            def __init__(self, index):
                self.index = index
                self.closed = False

            async def close(self):
                self.closed = True

        async def fake_context_factory(_playwright, _payload, _tmp_dir):
            context = FakeContext(len(created_contexts))
            created_contexts.append(context)
            return context

        async def fake_account_processor(context, account, _timeout_ms):
            processed.append((context.index, account.name))
            return playwright_checkin.AccountResult(name=account.name, success=True)

        results = await playwright_checkin.process_all_accounts(
            playwright=object(),
            payload={"headless": True},
            accounts=accounts,
            timeout_ms=30000,
            context_factory=fake_context_factory,
            account_processor=fake_account_processor,
        )

        self.assertEqual([(0, "账号A"), (1, "账号B")], processed)
        self.assertEqual(["账号A", "账号B"], [result.name for result in results])
        self.assertEqual(2, len(created_contexts))
        self.assertTrue(all(context.closed for context in created_contexts))

    async def test_wait_for_session_cookie_polls_until_callback_writes_cookie(self):
        class FakeContext:
            def __init__(self, snapshots):
                self._snapshots = snapshots
                self.calls = 0

            async def cookies(self, _urls):
                index = min(self.calls, len(self._snapshots) - 1)
                self.calls += 1
                return self._snapshots[index]

        # 前两次轮询拿不到 session，第三次 OAuth 回调写入后才出现。
        context = FakeContext(
            [
                [{"name": "acw_tc", "value": "waf"}],
                [{"name": "acw_tc", "value": "waf"}],
                [{"name": "session", "value": "abc"}],
            ]
        )

        found = await playwright_checkin.wait_for_session_cookie(
            context,
            "https://anyrouter.top",
            timeout_ms=5000,
            poll_interval_ms=1,
        )

        self.assertTrue(found)
        self.assertGreaterEqual(context.calls, 3)

    async def test_wait_for_session_cookie_times_out_without_cookie(self):
        class FakeContext:
            async def cookies(self, _urls):
                return [{"name": "acw_tc", "value": "waf"}]

        found = await playwright_checkin.wait_for_session_cookie(
            FakeContext(),
            "https://anyrouter.top",
            timeout_ms=5,
            poll_interval_ms=1,
        )

        self.assertFalse(found)

    async def test_session_cookie_present_ignores_empty_value(self):
        class FakeContext:
            async def cookies(self, _urls):
                return [{"name": "session", "value": ""}]

        present = await playwright_checkin.session_cookie_present(
            FakeContext(), "https://anyrouter.top"
        )

        self.assertFalse(present)


if __name__ == "__main__":
    unittest.main()
