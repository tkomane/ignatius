// Standalone actual-TerminalGuard actor. Include after renderer_console_viewport.
// The outside renderer supervisor remains the owner of the exact Job.
constexpr DWORD renderer_guard_vt_mask = ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING;
constexpr DWORD renderer_guard_mouse_mask = ENABLE_MOUSE_INPUT | ENABLE_WINDOW_INPUT | ENABLE_EXTENDED_FLAGS;
constexpr unsigned renderer_guard_mouse_bit = 1;
constexpr unsigned renderer_guard_alternate_bit = 4;

struct RendererGuardCase {
    std::wstring executable;
    std::wstring mode;
    unsigned bits{};
};

struct RendererGuardMeasurement {
    std::array<std::uint32_t, 14> native{};
    RendererPixelObservation pixels{};
    std::uint64_t capture_qpc{}, ack_qpc{};
};

struct RendererGuardReceipt {
    std::wstring mode;
    unsigned option_bits{};
    DWORD pid{}, expected_exit{}, exit_code{STILL_ACTIVE}, rejected_exit{STILL_ACTIVE};
    bool owned{}, exit_waited{}, restored_modes{}, finished{}, wrong_job_rejected{};
    std::vector<RendererGuardMeasurement> phases;
    std::array<std::uint32_t, 20> final{};
};

std::wstring renderer_guard_payload(const std::wstring& nonce, std::size_t phase) {
    require(phase == 1 || phase == 2 || phase == 5, "exact guard observation pixel phase");
    auto payload = renderer_tile(8, 10, 4, {16, 220, 144});
    payload += renderer_tile(2, 2, 6, renderer_phase_colors[phase]);
    payload += renderer_tile(8, 20, 1, {16, 16, 16});
    payload += L"\x1b[2;12H" + wide_ascii(w07::calibration_marker(ascii(nonce), phase));
    payload += L"\x1b[8;20H";
    return payload;
}

struct RendererGuardPhase {
    DWORD input{}, output{}, native_cursor_size{}, native_cursor_error{}, written{}, marker_read{};
    bool main_sentinel{}, native_cursor_ok{}, native_cursor_visible{};
    SMALL_RECT viewport{};
    std::uint64_t observed{};
};

RendererGuardPhase renderer_guard_phase(const std::wstring& nonce, std::size_t phase, HANDLE input,
                                        DWORD expected_input, DWORD seeded_output, const SMALL_RECT& fitted) {
    const auto payload = renderer_guard_payload(nonce, phase);
    const bool expected_main = renderer_main[phase];
    const auto active = renderer_open_output();
    RendererGuardPhase sample;
    require(GetConsoleMode(input, &sample.input) && GetConsoleMode(active.value, &sample.output),
            "read guard native console masks");
    sample.viewport = renderer_console_viewport(active.value);
    CONSOLE_CURSOR_INFO cursor{};
    sample.native_cursor_ok = GetConsoleCursorInfo(active.value, &cursor) != 0;
    sample.native_cursor_error = sample.native_cursor_ok ? ERROR_SUCCESS : GetLastError();
    if (sample.native_cursor_ok) {
        sample.native_cursor_size = cursor.dwSize;
        sample.native_cursor_visible = cursor.bVisible != FALSE;
    }
    // GetConsoleCursorInfo can follow a different buffer from the visible
    // renderer. Preserve its observation without substituting it for pixels.
    sample.main_sentinel = renderer_product_sentinel(active.value, nonce);
    require(sample.input == expected_input && (sample.output & renderer_guard_vt_mask) == renderer_guard_vt_mask
            && (!expected_main || sample.output == seeded_output),
            "actual guard cooked/raw/mouse and output masks match the seeded phase");
    require(sample.main_sentinel == expected_main
            && sample.viewport.Right == fitted.Right && sample.viewport.Bottom == fitted.Bottom,
            "actual guard main/alternate sentinel and fitted viewport");

    // CONOUT$ is reopened per phase. The shared observer handle is read-only,
    // so the marker writer separately selects the active output buffer.
    Handle writer(CreateFileW(L"CONOUT$", GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE,
                              nullptr, OPEN_EXISTING, 0, nullptr));
    require(static_cast<bool>(writer), "open the current guard marker writer");
    require(WriteConsoleW(writer.value, payload.data(), static_cast<DWORD>(payload.size()),
                          &sample.written, nullptr) != 0
            && sample.written == payload.size(), "write the complete guard content fence");
    const auto marker = wide_ascii(w07::calibration_marker(ascii(nonce), phase));
    std::wstring observed_marker(marker.size(), L'\0');
    sample.marker_read = renderer_read(active.value, COORD{11, 1}, observed_marker);
    require(observed_marker == marker && renderer_product_sentinel(active.value, nonce) == expected_main,
            "read back the exact guard content fence and untouched sentinel");
    sample.observed = ticks();
    return sample;
}

struct RendererGuardMarkers {
    std::size_t previous{}, panic{}, expected_error{}, hook_failure{};

    bool clear() const { return previous == 0 && panic == 0 && expected_error == 0 && hook_failure == 0; }
};

std::size_t renderer_guard_token(const std::wstring& text, std::wstring_view marker) {
    const auto first = text.find(marker);
    if (first == std::wstring::npos) return 0;
    require(text.find(marker, first + marker.size()) == std::wstring::npos, "guard marker occurs once");
    // Zero is reserved for absence. Offsets retain order and no console text.
    return first + 1;
}

RendererGuardMarkers renderer_guard_markers(HANDLE output, Clock::time_point deadline) {
    CONSOLE_SCREEN_BUFFER_INFO surface{};
    require(GetConsoleScreenBufferInfo(output, &surface) != 0, "read bounded guard marker viewport");
    const auto window = surface.srWindow;
    const auto width = window.Right - window.Left + 1;
    const auto height = window.Bottom - window.Top + 1;
    require(window.Left >= 0 && window.Top >= 0 && width >= 56 && width <= 240 && height >= 10 && height <= 120,
            "guard marker scan stays inside the owned console viewport");
    std::wstring text;
    text.reserve(static_cast<std::size_t>(width) * static_cast<std::size_t>(height));
    std::wstring row(static_cast<std::size_t>(width), L'\0');
    for (int y = window.Top; y <= window.Bottom; ++y) {
        require(Clock::now() < deadline, "guard console marker scan deadline");
        renderer_read(output, COORD{window.Left, static_cast<SHORT>(y)}, row);
        text += row;
    }
    CONSOLE_SCREEN_BUFFER_INFO after{};
    require(GetConsoleScreenBufferInfo(output, &after) != 0
            && after.srWindow.Left == window.Left && after.srWindow.Top == window.Top
            && after.srWindow.Right == window.Right && after.srWindow.Bottom == window.Bottom,
            "guard marker scan window stayed stable");
    return {renderer_guard_token(text, L"W07_RENDERER_PREVIOUS_HOOK"),
            renderer_guard_token(text, L"W07_RENDERER_SYNTHETIC_PANIC"),
            renderer_guard_token(text, L"W07_RENDERER_EXPECTED_ERROR"),
            renderer_guard_token(text, L"W07_RENDERER_HOOK_FAILED")};
}

void renderer_run_guard(const std::wstring& guard_exe, const std::wstring& scenario, unsigned bits,
                        const std::wstring& outer_pipe, const std::wstring& nonce, const std::wstring& work,
                        HANDLE input, DWORD seeded_input, DWORD seeded_output,
                        Clock::time_point work_deadline) {
    const bool panic = scenario == L"panic";
    const bool error = scenario == L"error";
    require(panic || error || scenario == L"normal", "exact guard scenario");
    require(bits <= 7, "exact guard option bits");
    require_renderer_nonce(nonce);
    require(outer_pipe == renderer_pipe_name(nonce), "exact outer guard control pipe");
    require(renderer_archive_baseline(seeded_input) == seeded_input
            && (seeded_output & renderer_guard_vt_mask) == renderer_guard_vt_mask, "guard actor's cooked/VT seed");
    const auto nonce_text = ascii(nonce);
    const auto baseline = renderer_open_output();
    const auto fitted = renderer_console_viewport(baseline.value);
    DWORD baseline_input{}, baseline_output{};
    require(GetConsoleMode(input, &baseline_input) && GetConsoleMode(baseline.value, &baseline_output)
            && baseline_input == seeded_input && baseline_output == seeded_output
            && renderer_product_sentinel(baseline.value, nonce), "observe the untouched guard seed before spawn");

    const auto phase_deadline = (std::min)(work_deadline, Clock::now() + 20s);
    const std::wstring inner_pipe = LR"(\\.\pipe\ignatius-w07-renderer-guard-)" + nonce;
    Security security;
    std::optional<Server> inner(std::in_place, inner_pipe, security, work_deadline);
    STARTUPINFOW startup{};
    startup.cb = sizeof(startup);
    auto child = create(guard_exe, {L"--renderer-case", scenario, std::to_wstring(bits), inner_pipe, nonce},
                        work, &startup, 0);
    try {
        const auto created = birth(child.process.value);
        const auto handle = static_cast<std::uint64_t>(reinterpret_cast<std::uintptr_t>(child.process.value));
        std::ostringstream registered;
        registered << "P " << nonce_text << ' ' << child.pid;
        renderer_stamp(registered, handle);
        registered << ' ' << created.dwLowDateTime << ' ' << created.dwHighDateTime;
        // ACK requires the outside supervisor to authenticate this exact
        // stable handle, executable, birth and Job while still suspended.
        send(outer_pipe, registered.str());
        require(Clock::now() < phase_deadline, "guard suspended-registration deadline");
        resume(child);

        const DWORD raw_input = (bits & renderer_guard_mouse_bit) != 0
            ? renderer_guard_mouse_mask : seeded_input & ~renderer_cooked_bits;
        const std::size_t acquired_phase = (bits & renderer_guard_alternate_bit) != 0 ? 2 : 1;
        bool before_clear = false;
        std::uint64_t restored_outer_ack{}, restored_inner_ack_begin{}, restored_inner_ack_end{};
        for (const auto phase : {acquired_phase, std::size_t{5}}) {
            const bool restored = phase == 5;
            inner->connect(phase_deadline);
            require(inner->client() == child.pid && GetProcessId(child.process.value) == child.pid
                    && WaitForSingleObject(child.process.value, 0) == WAIT_TIMEOUT
                    && renderer_product_attached(child.pid), "authenticate exact attached guard client");
            const auto reply = inner->read(phase_deadline);
            require(reply == "G " + nonce_text + (restored ? " restored" : " acquired"),
                    "exact inner guard nonce and phase order");
            const auto observation = renderer_guard_phase(nonce, phase, input,
                                                          restored ? seeded_input : raw_input, seeded_output, fitted);
            if (restored) {
                const auto output = renderer_open_output();
                before_clear = renderer_guard_markers(output.value, phase_deadline).clear();
                require(before_clear, "guard markers are absent while restored ACK is withheld");
            }
            std::ostringstream message;
            message << (restored ? "T " : "N ") << nonce_text << ' ' << phase
                    << ' ' << observation.input << ' ' << observation.output << ' ' << observation.main_sentinel
                    << ' ' << observation.native_cursor_ok << ' ' << observation.native_cursor_visible
                    << ' ' << observation.native_cursor_size << ' ' << observation.native_cursor_error
                    << ' ' << observation.written << ' ' << observation.marker_read
                    << ' ' << observation.viewport.Right + 1 << ' ' << observation.viewport.Bottom + 1;
            renderer_stamp(message, observation.observed);
            // The Rust child cannot leave its actual phase before the outside
            // supervisor has authenticated and sampled the rendered frame.
            send(outer_pipe, message.str());
            require(Clock::now() < phase_deadline && WaitForSingleObject(child.process.value, 0) == WAIT_TIMEOUT,
                    "guard is alive after outer phase ACK and before inner ACK");
            if (restored) {
                restored_outer_ack = ticks();
                restored_inner_ack_begin = ticks();
            }
            inner->ack(phase_deadline);
            if (restored) restored_inner_ack_end = ticks();
        }

        DWORD exit_code{};
        require(WaitForSingleObject(child.process.value, remaining(phase_deadline, 20000)) == WAIT_OBJECT_0
                && GetExitCodeProcess(child.process.value, &exit_code), "bounded wait for exact guard exit");
        const auto waited = ticks();
        const bool inner_quiescent = inner->quiescent();
        const auto inner_bytes = inner->bytes();
        // Two messages of 44 bytes, each with a four-byte ACK and two-byte K.
        require(inner_quiescent && inner_bytes == 100 && Clock::now() < phase_deadline,
                "exact guard inner connection exchanges and cleanup");
        inner.reset();
        const auto output = renderer_open_output();
        DWORD final_input{}, final_output{};
        require(GetConsoleMode(input, &final_input) && GetConsoleMode(output.value, &final_output),
                "read native masks after guard process exit");
        const bool main_sentinel = renderer_product_sentinel(output.value, nonce);
        const auto markers = renderer_guard_markers(output.value, phase_deadline);

        std::ostringstream finished;
        finished << "X " << nonce_text << ' ' << exit_code << ' ' << final_input << ' ' << final_output
                 << ' ' << main_sentinel << ' ' << 1 << ' ' << inner_quiescent << ' ' << inner_bytes
                 << ' ' << markers.previous << ' ' << markers.panic << ' ' << markers.expected_error
                 << ' ' << markers.hook_failure << ' ' << before_clear;
        renderer_stamp(finished, restored_outer_ack);
        renderer_stamp(finished, restored_inner_ack_begin);
        renderer_stamp(finished, restored_inner_ack_end);
        renderer_stamp(finished, waited);
        send(outer_pipe, finished.str());

        require(Clock::now() < work_deadline, "guard final receipt within the outer work deadline");
        require(final_input == seeded_input && final_output == seeded_output && main_sentinel,
                "completed guard retains native restored main baseline");
        require(exit_code == (panic ? 101UL : error ? 23UL : 0UL), "actual guard process exit matches the case");
        require(markers.hook_failure == 0
                && (panic ? markers.previous > 0 && markers.panic > markers.previous && markers.expected_error == 0
                          : error ? markers.expected_error > 0 && markers.previous == 0 && markers.panic == 0
                                  : markers.clear()),
                "ordered guard marker and default payload evidence");
    } catch (...) {
        // Only the known direct CreateProcess handle is used here. The
        // outside supervisor owns the exact Job and is the deadline fallback.
        if (WaitForSingleObject(child.process.value, 0) == WAIT_TIMEOUT) {
            (void)TerminateProcess(child.process.value, 125);
            (void)WaitForSingleObject(child.process.value, remaining(work_deadline, 500));
        }
        throw;
    }
}
