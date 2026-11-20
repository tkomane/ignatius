// Clipboard-free calibration of the owned Terminal's DEC mode 2004 reply.
enum class RendererPasteStatus : std::uint32_t {
    unsupported, enabled, reset, permanent_enabled, permanent_reset,
    timeout, malformed, input_not_quiet, console_error
};

constexpr std::wstring_view renderer_paste_prefix = L"\x1b[?2004;";
constexpr std::wstring_view renderer_paste_query = L"\x1b[?2004$p";
constexpr std::size_t renderer_paste_reply_units = renderer_paste_prefix.size() + 3;
constexpr DWORD renderer_paste_record_limit = 48;
constexpr DWORD renderer_paste_ambient_limit = 16;

struct RendererPasteReply {
    RendererPasteStatus status{RendererPasteStatus::timeout};
    DWORD records{}, characters{}, error{};
    DWORD ambient_focus{}, ambient_window{};
    bool quiet_before{}, quiet_after{};
};

struct RendererPasteStep {
    std::wstring_view bytes;
    RendererPasteStatus expected;
    bool main;
    unsigned input_profile; // 0 cooked, 1 raw without mouse, 2 raw with mouse
};

constexpr std::array<RendererPasteStep, 7> renderer_paste_steps{{
    {L"", RendererPasteStatus::reset, true, 0},
    {L"\x1b[?2004h", RendererPasteStatus::enabled, true, 1},
    {L"\x1b[?2004l", RendererPasteStatus::reset, true, 1},
    {L"\x1b[?1049h", RendererPasteStatus::reset, false, 2},
    {L"\x1b[?2004h", RendererPasteStatus::enabled, false, 2},
    {L"\x1b[?2004l", RendererPasteStatus::reset, false, 2},
    {L"\x1b[?1049l", RendererPasteStatus::reset, true, 0},
}};

DWORD renderer_paste_input(DWORD cooked, unsigned profile) {
    if (profile == 2) return renderer_guard_mouse_mask;
    if (profile == 1) return cooked & ~renderer_cooked_bits;
    return cooked;
}

struct RendererPasteReceipt {
    std::vector<std::array<std::uint32_t, 14>> observations;
    bool stopped{}, supported{};
};

void renderer_paste_write(HANDLE output, std::wstring_view bytes) {
    if (bytes.empty()) return;
    DWORD written{};
    require(WriteConsoleW(output, bytes.data(), static_cast<DWORD>(bytes.size()), &written, nullptr)
            && written == bytes.size(), "write the complete owned paste calibration VT control");
}

bool renderer_paste_reply_unit(std::size_t index, wchar_t unit) {
    if (index < renderer_paste_prefix.size()) return unit == renderer_paste_prefix[index];
    if (index == renderer_paste_prefix.size()) return unit >= L'0' && unit <= L'4';
    return (index == renderer_paste_prefix.size() + 1 && unit == L'$')
        || (index == renderer_paste_prefix.size() + 2 && unit == L'y');
}

RendererPasteReply renderer_observe_paste(HANDLE input, HANDLE output, Clock::time_point overall) {
    constexpr DWORD quiet_ms = 40;
    const auto deadline = (std::min)(overall, Clock::now() + 750ms);
    RendererPasteReply result;
    const auto native_error = [&result] {
        result.error = GetLastError();
        result.status = RendererPasteStatus::console_error;
        return result;
    };
    DWORD pending{};
    // Pre-query housekeeping is permitted only before the first request or
    // after the preceding phase already completed an exact, quiet exchange.
    while (remaining(deadline, quiet_ms) == quiet_ms) {
        const auto before = WaitForSingleObject(input, quiet_ms);
        if (before == WAIT_FAILED || !GetNumberOfConsoleInputEvents(input, &pending)) return native_error();
        if (before == WAIT_TIMEOUT && pending == 0) {
            result.quiet_before = true;
            break;
        }
        const DWORD room = renderer_paste_ambient_limit - result.ambient_focus - result.ambient_window;
        result.status = RendererPasteStatus::input_not_quiet;
        if (pending == 0 || pending > room || (before != WAIT_OBJECT_0 && before != WAIT_TIMEOUT)) return result;
        std::array<INPUT_RECORD, renderer_paste_ambient_limit + 1> peeked{};
        DWORD count{};
        if (!PeekConsoleInputW(input, peeked.data(), room + 1, &count)) return native_error();
        if (count < pending || count > room) return result;
        for (DWORD i = 0; i < count; ++i) {
            const auto type = peeked[i].EventType;
            if (type != FOCUS_EVENT && type != WINDOW_BUFFER_SIZE_EVENT) return result;
        }
        // The actor is the sole reader of this owned FIFO. Read only the
        // checked prefix so later arrivals remain queued for the next peek.
        std::array<INPUT_RECORD, renderer_paste_ambient_limit> consumed{};
        DWORD read{};
        if (!ReadConsoleInputW(input, consumed.data(), count, &read)) return native_error();
        if (read != count) return result;
        for (DWORD i = 0; i < read; ++i) {
            const auto type = consumed[i].EventType;
            if (type != peeked[i].EventType) return result;
            if (type == FOCUS_EVENT) ++result.ambient_focus;
            else ++result.ambient_window;
        }
        result.status = RendererPasteStatus::timeout;
    }
    if (!result.quiet_before) return result;
    renderer_paste_write(output, renderer_paste_query);

    wchar_t state{};
    // The actor is the only reader in the fresh owned pane. One query is
    // outstanding. Check the available event count before each bounded read.
    while (Clock::now() < deadline) {
        const bool complete = result.characters == renderer_paste_reply_units;
        const DWORD wait_ms = remaining(deadline, complete ? quiet_ms : 750);
        if (wait_ms == 0 || (complete && wait_ms < quiet_ms)) break;
        const auto wait = WaitForSingleObject(input, wait_ms);
        if (wait == WAIT_FAILED || !GetNumberOfConsoleInputEvents(input, &pending)) return native_error();
        if (wait == WAIT_TIMEOUT && pending == 0) {
            if (complete) {
                result.status = static_cast<RendererPasteStatus>(state - L'0');
                result.quiet_after = true;
            }
            return result;
        }
        if (wait != WAIT_OBJECT_0 || pending == 0 || pending > renderer_paste_record_limit - result.records) {
            result.status = RendererPasteStatus::input_not_quiet;
            return result;
        }
        std::array<INPUT_RECORD, renderer_paste_record_limit> events{};
        DWORD count{};
        if (!ReadConsoleInputW(input, events.data(), pending, &count)) return native_error();
        if (count != pending) {
            result.status = RendererPasteStatus::input_not_quiet;
            return result;
        }
        result.records += count;
        for (DWORD i = 0; i < count; ++i) {
            const auto& event = events[i];
            if (event.EventType != KEY_EVENT) {
                result.status = RendererPasteStatus::input_not_quiet;
                return result;
            }
            const auto& key = event.Event.KeyEvent;
            if (!key.bKeyDown) continue;
            for (WORD repeat = 0; repeat < key.wRepeatCount; ++repeat) {
                const auto index = result.characters;
                const wchar_t unit = key.uChar.UnicodeChar;
                if (!renderer_paste_reply_unit(index, unit)) {
                    result.status = complete ? RendererPasteStatus::input_not_quiet
                                             : RendererPasteStatus::malformed;
                    return result;
                }
                if (index == renderer_paste_prefix.size()) state = unit;
                ++result.characters;
            }
        }
    }
    return result;
}

void renderer_run_paste(const std::wstring& pipe, const std::wstring& nonce, HANDLE input,
                        DWORD cooked, DWORD seeded_output, Clock::time_point deadline) {
    const auto nonce_text = ascii(nonce);
    bool failed = false, main = true;
    for (std::size_t i = 0; i < renderer_paste_steps.size(); ++i) {
        const auto& step = renderer_paste_steps[i];
        const DWORD expected_input = renderer_paste_input(cooked, step.input_profile);
        DWORD actual_input{};
        require(GetConsoleMode(input, &actual_input), "read owned paste calibration input mode");
        if (actual_input != expected_input) {
            require(SetConsoleMode(input, expected_input) && GetConsoleMode(input, &actual_input)
                    && actual_input == expected_input, "calibrate the exact owned classic input mode");
        }
        Handle writer(CreateFileW(L"CONOUT$", GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE,
                                  nullptr, OPEN_EXISTING, 0, nullptr));
        require(static_cast<bool>(writer), "open the active owned paste calibration output");
        renderer_paste_write(writer.value, step.bytes);
        main = step.main;
        Handle active(CreateFileW(L"CONOUT$", GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE,
                                  nullptr, OPEN_EXISTING, 0, nullptr));
        DWORD actual_output{};
        require(active && GetConsoleMode(active.value, &actual_output)
                && (actual_output & renderer_guard_vt_mask) == renderer_guard_vt_mask
                && (!main || actual_output == seeded_output)
                && renderer_product_sentinel(active.value, nonce) == main,
                "paste calibration is in the known active main or alternate buffer");
        const auto reply = renderer_observe_paste(input, active.value, deadline);
        DWORD after_input{};
        require(GetConsoleMode(input, &after_input) && after_input == actual_input,
                "paste query and input observation preserve the native mode");
        std::ostringstream observed;
        observed << "B " << nonce_text << ' ' << i << ' ' << static_cast<unsigned>(reply.status)
                 << ' ' << actual_input << ' ' << actual_output << ' ' << main
                 << ' ' << reply.characters << ' ' << reply.records
                 << ' ' << reply.quiet_before << ' ' << reply.quiet_after << ' ' << reply.error
                 << ' ' << reply.ambient_focus << ' ' << reply.ambient_window;
        renderer_stamp(observed, ticks());
        send(pipe, observed.str());
        if (reply.status != step.expected || !reply.quiet_before || !reply.quiet_after) {
            failed = true;
            break; // A late reply can never be consumed by a subsequent query.
        }
    }
    if (failed) {
        Handle output(CreateFileW(L"CONOUT$", GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE,
                                  nullptr, OPEN_EXISTING, 0, nullptr));
        require(static_cast<bool>(output), "open owned paste calibration cleanup output");
        renderer_paste_write(output.value, main ? L"\x1b[?2004l" : L"\x1b[?2004l\x1b[?1049l");
    }
    require(SetConsoleMode(input, cooked), "release test-only raw input modes before paste calibration teardown");
}

constexpr std::array<const char*, 9> renderer_paste_status_names{
    "unsupported", "enabled", "reset", "permanent-enabled", "permanent-reset",
    "timeout", "malformed", "input-not-quiet", "console-error",
};

void emit_renderer_paste(const RendererPasteReceipt& receipt) {
    std::cout << ",\"paste_calibration\":{\"supported\":" << (receipt.supported ? "true" : "false")
              << ",\"observations\":[";
    bool first = true;
    for (const auto& n : receipt.observations) {
        if (!first) std::cout << ',';
        first = false;
        std::cout << "{\"index\":" << n[0] << ",\"reply\":\"" << renderer_paste_status_names[n[1]]
                  << "\",\"input\":" << n[2] << ",\"output\":" << n[3]
                  << ",\"main\":" << (n[4] ? "true" : "false")
                  << ",\"reply_units\":" << n[5] << ",\"input_records\":" << n[6]
                  << ",\"quiet_before\":" << (n[7] ? "true" : "false")
                  << ",\"quiet_after\":" << (n[8] ? "true" : "false")
                  << ",\"native_error\":" << n[9]
                  << ",\"ambient_focus_before\":" << n[10] << ",\"ambient_window_before\":" << n[11]
                  << ",\"qpc\":" << renderer_stamp(n[12], n[13]) << '}';
    }
    std::cout << "]}";
}
