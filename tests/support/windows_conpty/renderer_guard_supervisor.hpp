// Standalone Rust Guard receipt validation, using the same owned HWND/Job.
DWORD renderer_guard_exit(const std::wstring& mode) {
    if (mode == L"normal") return 0;
    if (mode == L"error") return 23;
    require(mode == L"panic", "exact normal, error or panic Guard selection");
    return 101;
}

void renderer_guard_accept_paste(RendererGuardReceipt& guard, Server& server,
        const std::vector<std::string_view>& fields, const RendererPhase& baseline,
        HANDLE child, HANDLE job, Clock::time_point deadline) {
    const auto count = guard.paste_observations.size();
    require(guard.paste_required && count < 3, "only the explicit Guard paste case may send three observations");
    const bool acquired = count == 1;
    const bool alternate = (guard.option_bits & renderer_guard_alternate_bit) != 0;
    const bool main = !acquired || !alternate;
    if (count == 0) {
        require(!child && !guard.owned && guard.phases.empty(), "Guard paste baseline precedes child launch");
    } else {
        require(child && guard.owned && GetProcessId(child) == guard.pid && WaitForSingleObject(child, 0) == WAIT_TIMEOUT
                && guard.phases.size() == count - 1, "Guard paste follows the authenticated held phase sequence");
        exact_job(child, job);
    }
    const auto n = renderer_numbers<14>(fields);
    const auto phase = acquired ? (alternate ? 2U : 1U) : (count == 0 ? 0U : 5U);
    const auto input = acquired ? renderer_paste_input(baseline.input, guard.option_bits & renderer_guard_mouse_bit ? 2 : 1)
        : baseline.input;
    const auto prior = count == 2 ? guard.phases.back().ack_qpc : baseline.ack_qpc;
    const auto observed = renderer_stamp(n[12], n[13]);
    require(n[0] == phase && renderer_paste_bounded(n) && n[2] == input && n[4] == (main ? 1U : 0U)
            && (n[3] & renderer_guard_vt_mask) == renderer_guard_vt_mask && (!main || n[3] == baseline.output)
            && observed >= prior && observed <= ticks(), "bounded Guard paste state tied to the native console phase");
    guard.paste_observations.push_back(n);
    const auto expected = acquired && (guard.option_bits & renderer_guard_paste_bit) != 0
        ? RendererPasteStatus::enabled : RendererPasteStatus::reset;
    require(renderer_paste_exact(n, expected), "exact renderer paste state and quiet input for this Guard phase");
    server.ack(deadline);
}

void renderer_guard_accept_phase(RendererGuardReceipt& guard, Server& server,
        const std::vector<std::string_view>& fields, const RendererPhase& baseline,
        const RendererWindow& renderer, HANDLE job, HANDLE child,
        const std::wstring& terminal, const std::wstring& title, const std::wstring& nonce,
        const std::filesystem::path& folder, Clock::time_point deadline) {
    require(guard.owned && WaitForSingleObject(child, 0) == WAIT_TIMEOUT,
            "actual Guard child is held in the measured phase");
    const bool restored = fields[0] == "T";
    const bool alternate = (guard.option_bits & renderer_guard_alternate_bit) != 0;
    const auto expected_index = restored ? 5U : alternate ? 2U : 1U;
    const bool expected_main = restored || !alternate;
    const DWORD expected_input = restored ? baseline.input
        : (guard.option_bits & renderer_guard_mouse_bit) != 0
            ? renderer_guard_mouse_mask : baseline.input & ~renderer_cooked_bits;
    require(guard.phases.size() == (restored ? 1U : 0U), "Guard phases are ordered acquired then restored");
    RendererGuardMeasurement phase;
    phase.native = renderer_numbers<14>(fields);
    guard.phases.push_back(phase);
    auto& sampled = guard.phases.back();
    const auto& n = sampled.native;
    const auto prior = restored ? guard.phases.front().ack_qpc : baseline.ack_qpc;
    const auto observed = renderer_stamp(n[12], n[13]);
    require(n[0] == expected_index && n[1] == expected_input
            && (n[2] & renderer_guard_vt_mask) == renderer_guard_vt_mask
            && (!expected_main || n[2] == baseline.output) && n[3] == (expected_main ? 1U : 0U)
            && n[4] <= 1 && n[5] <= 1 && (n[4] ? n[6] >= 1 && n[6] <= 100 && n[7] == 0 : n[6] == 0)
            && n[8] == renderer_guard_payload(nonce, expected_index).size()
            && n[9] == w07::calibration_marker(ascii(nonce), expected_index).size()
            && n[10] >= 56 && n[10] <= 200 && n[11] >= 10 && n[11] <= 80
            && observed >= prior && observed <= ticks(), "bounded actual Guard native observation");
    if (guard.paste_required) {
        require(guard.paste_observations.size() == (restored ? 3U : 2U),
                "the held Guard phase supplied its authenticated paste state before renderer capture");
        const auto& paste = guard.paste_observations.back();
        require(paste[0] == n[0] && paste[2] == n[1] && paste[3] == n[2] && paste[4] == n[3]
                && renderer_stamp(paste[12], paste[13]) >= observed,
                "the Guard query follows the native content fence in the same held phase");
    }
    if (restored) {
        require(n[10] == guard.phases.front().native[10] && n[11] == guard.phases.front().native[11],
                "Guard console viewport is stable");
    }
    verify_renderer_window(renderer, job, terminal, title);
    sampled.pixels = observe_renderer_frame(renderer.hwnd, expected_index, restored, expected_main,
        deadline, folder / (restored ? L"guard-restored.bmp" : L"guard-acquired.bmp"));
    verify_renderer_window(renderer, job, terminal, title);
    sampled.capture_qpc = ticks();
    require(sampled.capture_qpc >= observed && Clock::now() < deadline,
            "Guard renderer proof precedes release of this phase");
    sampled.ack_qpc = ticks();
    server.ack(deadline);
}

void renderer_guard_accept_exit(RendererGuardReceipt& guard, Server& server,
        const std::vector<std::string_view>& fields, const RendererPhase& baseline,
        HANDLE child, Clock::time_point deadline) {
    require(guard.owned && !guard.finished && guard.phases.size() == 2
            && (!guard.paste_required || guard.paste_observations.size() == 3),
            "Guard exit follows both native phases and any required paste observations");
    guard.final = renderer_numbers<20>(fields);
    const auto& n = guard.final;
    guard.exit_waited = WaitForSingleObject(child, 0) == WAIT_OBJECT_0;
    require(guard.exit_waited && GetExitCodeProcess(child, &guard.exit_code)
            && guard.exit_code == n[0] && guard.exit_code == guard.expected_exit
            && n[4] == 1 && n[5] == 1 && n[6] == 100,
            "actual child exit and exact closed Guard side-channel exchanges");
    const auto outer_ack = renderer_stamp(n[12], n[13]);
    const auto inner_begin = renderer_stamp(n[14], n[15]);
    const auto inner_end = renderer_stamp(n[16], n[17]);
    const auto waited = renderer_stamp(n[18], n[19]);
    require(guard.phases.back().ack_qpc <= outer_ack && outer_ack <= inner_begin
            && inner_begin <= inner_end && inner_end <= waited && waited <= ticks(),
            "native restoration was measured before the released previous hook and exit");
    const bool panic = guard.mode == L"panic";
    const bool error = guard.mode == L"error";
    require(n[7] <= 28800 && n[8] <= 28800 && n[9] <= 28800 && n[10] == 0 && n[11] == 1
            && (panic ? n[7] != 0 && n[8] > n[7] && n[9] == 0
                : n[7] == 0 && n[8] == 0 && (n[9] != 0) == error),
            "exact ordered controlled Guard error and uncaught panic markers");
    guard.restored_modes = n[1] == baseline.input && n[2] == baseline.output && n[3] == 1;
    require(guard.restored_modes, "native Guard seed is still restored after actual child exit");
    server.ack(deadline);
    guard.finished = true;
}

void emit_renderer_guard(const RendererGuardReceipt& guard) {
    const auto truth = [](bool value) { return value ? "true" : "false"; };
    std::cout << ",\"guard\":{\"mode\":\"" << ascii(guard.mode) << "\",\"option_bits\":" << guard.option_bits
              << ",\"pid\":" << guard.pid << ",\"owned\":" << truth(guard.owned)
              << ",\"expected_exit\":" << guard.expected_exit << ",\"exit_code\":" << guard.exit_code
              << ",\"exit_waited\":" << truth(guard.exit_waited)
              << ",\"restored_modes\":" << truth(guard.restored_modes)
              << ",\"wrong_job_rejected\":" << truth(guard.wrong_job_rejected)
              << ",\"wrong_job_control_exit\":" << guard.rejected_exit
              << ",\"paste_state\":\"" << (guard.paste_required
                     ? (guard.finished ? "verified-native" : "incomplete") : "open-not-measured") << '"';
    if (guard.paste_required) {
        std::cout << ",\"paste_observations\":";
        emit_renderer_paste_observations(guard.paste_observations);
    }
    std::cout << ",\"phases\":[";
    for (std::size_t i = 0; i < guard.phases.size(); ++i) {
        if (i > 0) std::cout << ',';
        const auto& p = guard.phases[i];
        const auto& n = p.native;
        std::cout << "{\"index\":" << n[0] << ",\"input\":" << n[1] << ",\"output\":" << n[2]
                  << ",\"native_main\":" << truth(n[3] == 1)
                  << ",\"classic_cursor_ok\":" << truth(n[4] == 1)
                  << ",\"classic_cursor_visible\":" << truth(n[5] == 1)
                  << ",\"classic_cursor_size\":" << n[6] << ",\"classic_cursor_error\":" << n[7]
                  << ",\"marker_written\":" << n[8] << ",\"marker_read\":" << n[9]
                  << ",\"columns\":" << n[10] << ",\"rows\":" << n[11]
                  << ",\"native_qpc\":" << renderer_stamp(n[12], n[13])
                  << ",\"capture_qpc\":" << p.capture_qpc << ",\"ack_qpc\":" << p.ack_qpc
                  << ",\"rendered_cursor_pixels\":" << p.pixels.cursor_pixels
                  << ",\"rendered_cursor_area\":" << p.pixels.cursor_area
                  << ",\"rendered_sentinel_pixels\":" << p.pixels.sentinel_pixels
                  << ",\"stable_frames\":" << p.pixels.stable_frames << '}';
    }
    const auto& n = guard.final;
    std::cout << "],\"final_observation\":{\"observed\":" << truth(guard.finished)
              << ",\"input\":" << n[1] << ",\"output\":" << n[2] << ",\"main\":" << truth(n[3] == 1)
              << ",\"inner_control_quiescent\":" << truth(n[5] == 1) << ",\"inner_control_bytes\":" << n[6]
              << ",\"previous_hook_offset\":" << n[7] << ",\"panic_payload_offset\":" << n[8]
              << ",\"expected_error_offset\":" << n[9] << ",\"hook_failure_offset\":" << n[10]
              << ",\"markers_absent_before_ack\":" << truth(n[11] == 1)
              << ",\"actor_outer_ack_qpc\":" << renderer_stamp(n[12], n[13])
              << ",\"actor_inner_ack_begin_qpc\":" << renderer_stamp(n[14], n[15])
              << ",\"actor_inner_ack_end_qpc\":" << renderer_stamp(n[16], n[17])
              << ",\"actor_wait_qpc\":" << renderer_stamp(n[18], n[19]) << "}}";
}
