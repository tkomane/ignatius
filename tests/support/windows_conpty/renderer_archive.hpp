// Archived application smoke in the already owned Windows Terminal console.
constexpr DWORD renderer_cooked_bits = ENABLE_PROCESSED_INPUT | ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT;

DWORD renderer_archive_baseline(DWORD observed) {
    return (observed | renderer_cooked_bits | ENABLE_EXTENDED_FLAGS)
        & ~(ENABLE_MOUSE_INPUT | ENABLE_WINDOW_INPUT | ENABLE_QUICK_EDIT_MODE | ENABLE_VIRTUAL_TERMINAL_INPUT);
}

struct RendererLoopback {
    SOCKET socket_handle{INVALID_SOCKET};
    bool initialized{};
    RendererLoopback() = default;
    RendererLoopback(const RendererLoopback&) = delete;
    RendererLoopback& operator=(const RendererLoopback&) = delete;
    ~RendererLoopback() {
        if (socket_handle != INVALID_SOCKET) closesocket(socket_handle);
        if (initialized) WSACleanup();
    }
    std::wstring reserve() {
        WSADATA data{};
        require(WSAStartup(MAKEWORD(2, 2), &data) == 0, "initialize the owned loopback fixture");
        initialized = true;
        socket_handle = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
        require(socket_handle != INVALID_SOCKET, "create the owned loopback socket");
        const BOOL exclusive = TRUE;
        require(setsockopt(socket_handle, SOL_SOCKET, SO_EXCLUSIVEADDRUSE,
                           reinterpret_cast<const char*>(&exclusive), static_cast<int>(sizeof(exclusive))) == 0,
                "keep the loopback endpoint exclusive");
        sockaddr_in address{};
        address.sin_family = AF_INET;
        address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
        require(bind(socket_handle, reinterpret_cast<const sockaddr*>(&address), static_cast<int>(sizeof(address))) == 0,
                "reserve a synthetic loopback port");
        int length = static_cast<int>(sizeof(address));
        require(getsockname(socket_handle, reinterpret_cast<sockaddr*>(&address), &length) == 0
                && length == static_cast<int>(sizeof(address)) && address.sin_port != 0,
                "read the owned loopback fixture port");
        return std::to_wstring(ntohs(address.sin_port));
    }
};

bool renderer_product_attached(DWORD child) {
    DWORD members[128]{};
    const auto count = GetConsoleProcessList(members, static_cast<DWORD>(std::size(members)));
    require(count > 0 && count <= std::size(members), "bounded product console process list");
    return std::find(members, members + count, child) != members + count
        && std::find(members, members + count, GetCurrentProcessId()) != members + count;
}

Handle renderer_open_output() {
    Handle output(CreateFileW(L"CONOUT$", GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE,
                              nullptr, OPEN_EXISTING, 0, nullptr));
    require(static_cast<bool>(output), "reopen the active owned console output");
    return output;
}

bool renderer_product_heading(HANDLE output) {
    CONSOLE_SCREEN_BUFFER_INFO surface{};
    require(GetConsoleScreenBufferInfo(output, &surface) != 0, "read product console dimensions");
    const auto columns = surface.srWindow.Right - surface.srWindow.Left + 1;
    require(columns >= 80 && columns <= 200, "bound the full-layout product heading read");
    std::wstring heading(static_cast<std::size_t>(columns), L'\0');
    renderer_read(output, COORD{surface.srWindow.Left, surface.srWindow.Top}, heading);
    return heading.find(L"Ignatius") != std::wstring::npos;
}

bool renderer_product_sentinel(HANDLE output, const std::wstring& nonce) {
    const auto sentinel = L"W07-" + nonce;
    std::wstring seen(sentinel.size(), L'\0');
    renderer_read(output, COORD{11, 3}, seen);
    return seen == sentinel;
}

void renderer_product_quit(HANDLE input) {
    INPUT_RECORD keys[2]{};
    keys[0].EventType = KEY_EVENT;
    auto& press = keys[0].Event.KeyEvent;
    press.bKeyDown = TRUE;
    press.wRepeatCount = 1;
    press.wVirtualKeyCode = 'Q';
    press.uChar.UnicodeChar = L'q';
    press.dwControlKeyState = LEFT_CTRL_PRESSED;
    keys[1] = keys[0];
    keys[1].Event.KeyEvent.bKeyDown = FALSE;
    DWORD written{};
    require(WriteConsoleInputW(input, keys, static_cast<DWORD>(std::size(keys)), &written) != 0
            && written == std::size(keys), "send Ctrl+Q to the exact owned ConPTY input");
}

void renderer_run_archive(const std::wstring& archive, const std::wstring& pipe,
                          const std::wstring& nonce, const std::wstring& work, HANDLE input) {
    const auto nonce_text = ascii(nonce);
    const auto deadline = Clock::now() + 20s;
    RendererLoopback loopback;
    const auto endpoint = L"postgres://fixture@127.0.0.1:" + loopback.reserve()
        + L"/renderer_fixture?sslmode=disable&connect_timeout=2";
    STARTUPINFOW startup{};
    startup.cb = sizeof(startup);
    auto client = create(archive, {L"connect", L"--no-history", endpoint}, work, &startup, 0);
    try {
        const auto created = birth(client.process.value);
        const auto handle = static_cast<std::uint64_t>(reinterpret_cast<std::uintptr_t>(client.process.value));
        std::ostringstream register_client;
        register_client << "P " << nonce_text << ' ' << client.pid;
        renderer_stamp(register_client, handle);
        register_client << ' ' << created.dwLowDateTime << ' ' << created.dwHighDateTime;
        send(pipe, register_client.str());
        resume(client);

        bool active = false;
        DWORD input_mode{}, output_mode{}, observations{}, early_exit{STILL_ACTIVE};
        while (Clock::now() < deadline) {
            const auto wait = WaitForSingleObject(client.process.value, 0);
            if (wait == WAIT_OBJECT_0) {
                require(GetExitCodeProcess(client.process.value, &early_exit), "read early archived client exit");
                break;
            }
            require(wait == WAIT_TIMEOUT, "observe owned archived client startup");
            if (renderer_product_attached(client.pid)) {
                observations |= 1;
                const auto output = renderer_open_output();
                require(GetConsoleMode(input, &input_mode) && GetConsoleMode(output.value, &output_mode),
                        "read native archive startup modes");
                const bool raw = (input_mode & renderer_cooked_bits) == 0;
                const bool alternate = !renderer_product_sentinel(output.value, nonce);
                const bool heading = renderer_product_heading(output.value);
                observations |= (raw ? 2U : 0U) | (alternate ? 4U : 0U) | (heading ? 8U : 0U);
                if (raw && alternate && heading) {
                    active = true;
                    break;
                }
            }
            Sleep(remaining(deadline, 20));
        }
        if (!active) {
            std::ostringstream partial;
            partial << "E " << nonce_text << ' ' << observations << ' ' << input_mode << ' '
                    << output_mode << ' ' << early_exit;
            send(pipe, partial.str());
            throw std::runtime_error("archived client did not reach the native startup checkpoint");
        }
        std::ostringstream acquired;
        acquired << "N " << nonce_text << ' ' << input_mode << ' ' << output_mode;
        renderer_stamp(acquired, ticks());
        send(pipe, acquired.str());

        require(renderer_product_attached(client.pid)
                && WaitForSingleObject(client.process.value, 0) == WAIT_TIMEOUT,
                "owned archived client still has the authenticated console before Ctrl+Q");
        renderer_product_quit(input);
        DWORD exit_code{};
        require(WaitForSingleObject(client.process.value, remaining(deadline, 20000)) == WAIT_OBJECT_0
                && GetExitCodeProcess(client.process.value, &exit_code), "bounded archived client wait");
        const auto restored = renderer_open_output();
        require(GetConsoleMode(input, &input_mode) && GetConsoleMode(restored.value, &output_mode),
                "read archived client restored native masks");
        const bool sentinel = renderer_product_sentinel(restored.value, nonce);
        std::ostringstream after;
        after << "T " << nonce_text << ' ' << input_mode << ' ' << output_mode << ' ' << exit_code << ' ' << sentinel;
        renderer_stamp(after, ticks());
        send(pipe, after.str());
    } catch (...) {
        // This handle is from the actor's own CreateProcess. The outside Job
        // owner remains the fallback if termination or the bounded wait fails.
        if (WaitForSingleObject(client.process.value, 0) == WAIT_TIMEOUT) {
            TerminateProcess(client.process.value, 125);
            WaitForSingleObject(client.process.value, 500);
        }
        throw;
    }
}

struct RendererWindowProof {
    std::array<std::int64_t, 2> compositor_times{};
    bool foreground{};
};

struct RendererArchiveReceipt {
    DWORD pid{}, acquired_input{}, acquired_output{}, restored_input{}, restored_output{}, exit_code{STILL_ACTIVE};
    USHORT process_machine{}, native_machine{};
    bool machine_type_fallback{};
    std::uint64_t acquired_qpc{}, acquired_capture_qpc{}, restored_qpc{}, restored_capture_qpc{};
    bool child_owned{}, waited{}, raw_alternate{}, restored_sentinel{}, restored_modes{};
    int active_width{}, active_height{};
    std::size_t active_surface_pixels{}, active_focus_pixels{};
    DWORD startup_facts{}, startup_last_input{}, startup_last_output{}, startup_early_exit{STILL_ACTIVE};
    RendererPixelObservation restored_pixels{};
    bool wgc_software_device{};
    RendererWindowProof wgc_baseline{}, wgc_active{}, wgc_restored{};
};

Handle renderer_register_child(HANDLE actor, HANDLE job, const std::wstring& executable,
                               const std::array<std::uint32_t, 5>& data) {
    const auto reported_handle = renderer_stamp(data[1], data[2]);
    HANDLE copied{};
    require(data[0] != 0 && reported_handle != 0
            && DuplicateHandle(actor, reinterpret_cast<HANDLE>(static_cast<std::uintptr_t>(reported_handle)),
                               GetCurrentProcess(), &copied, PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE,
                               FALSE, 0) != 0, "duplicate the actor's exact suspended child handle");
    Handle owned(copied);
    exact_job(owned.value, job);
    const auto created = birth(owned.value);
    require(GetProcessId(owned.value) == data[0] && created.dwLowDateTime == data[3]
            && created.dwHighDateTime == data[4] && same_path(image(owned.value), executable)
            && WaitForSingleObject(owned.value, 0) == WAIT_TIMEOUT,
            "authenticate the suspended child executable in the exact renderer Job");
    return owned;
}

Handle renderer_register_archive(HANDLE actor, HANDLE job, const std::wstring& archive,
                                 const std::array<std::uint32_t, 5>& data, RendererArchiveReceipt& receipt) {
    auto owned = renderer_register_child(actor, job, archive, data);
    require(IsWow64Process2(owned.value, &receipt.process_machine, &receipt.native_machine) != 0,
            "record the archived process and native OS architectures");
    if (receipt.native_machine == IMAGE_FILE_MACHINE_ARM64
        && receipt.process_machine == IMAGE_FILE_MACHINE_UNKNOWN) {
        PROCESS_MACHINE_INFORMATION machine{};
        require(GetProcessInformation(owned.value, ProcessMachineTypeInfo, &machine,
                    static_cast<DWORD>(sizeof(machine))) != 0,
                "record the machine type of the owned x64 process on ARM64");
        receipt.process_machine = machine.ProcessMachine;
        receipt.machine_type_fallback = true;
    }
    require((receipt.native_machine == IMAGE_FILE_MACHINE_AMD64
                && receipt.process_machine == IMAGE_FILE_MACHINE_UNKNOWN)
            || (receipt.native_machine == IMAGE_FILE_MACHINE_ARM64
                && receipt.process_machine == IMAGE_FILE_MACHINE_AMD64),
            "x64 archive is native on x64 or emulated on ARM64");
    receipt.child_owned = true;
    receipt.pid = data[0];
    return owned;
}

void renderer_capture_archive(HWND window, const std::filesystem::path& path, Clock::time_point deadline,
                              RendererArchiveReceipt& receipt) {
    using namespace renderer_pixels_detail;
    PixelCoordinates coordinates;
    std::optional<RECT> previous_verified;
    while (Clock::now() < deadline) {
        const auto region = visible_client(window);
        const auto frame = capture(region);
        require(same_rectangle(region, visible_client(window)), "owned archive capture region stayed fixed");
        const Box client{0, 0, frame.width, frame.height};
        require(frame.width >= 450 && frame.width <= 1024 && frame.height >= 260 && frame.height <= 768,
                "owned archive capture size stays bounded");
        // The archived client's isolated config forces these Material pane
        // and focus tokens. Require real foreground pixels as well as the
        // absence of the calibrated main-screen markers.
        const auto surface = count_color(frame, client, {0x19, 0x1c, 0x22});
        const auto focus = count_color(frame, client, {0x8a, 0xb4, 0xf8});
        const bool verified = surface >= 2048 && focus >= 24
            && find_anchor(frame).count == 0 && count_color(frame, client, sentinel_color) == 0;
        if (verified && previous_verified && same_rectangle(*previous_verified, region)) {
            coordinates.restore();
            write_crop(path, client, cropped_pixels(frame, client), deadline);
            receipt.active_width = frame.width;
            receipt.active_height = frame.height;
            receipt.active_surface_pixels = surface;
            receipt.active_focus_pixels = focus;
            return;
        }
        previous_verified = verified ? std::make_optional(region) : std::nullopt;
        Sleep(remaining(deadline, 45));
    }
    throw std::runtime_error("product renderer did not display its Material pixels in the alternate screen");
}

#ifdef IGNATIUS_RENDERER_WINDOW_CAPTURE
struct RendererWindowFrame {
    renderer_pixels_detail::Frame pixels;
    ignatius_wgc::Rectangle bounds;
    std::uint32_t generation;
    std::int64_t compositor_time;
    bool foreground;
};

RendererWindowFrame renderer_window_frame(ignatius_wgc::WindowCapture& capture, Clock::time_point deadline,
                                          std::int64_t fence) {
    using namespace renderer_pixels_detail;
    const auto source = capture.next(deadline, fence);
    require(source.width >= 450 && source.width <= 1024 && source.height >= 260 && source.height <= 768,
            "owned WGC archive client dimensions");
    Frame frame{source.width, source.height, std::vector<Pixel>(source.pixels.size())};
    std::transform(source.pixels.begin(), source.pixels.end(), frame.pixels.begin(),
                   [](const ignatius_wgc::Pixel& pixel) { return Pixel{pixel.b, pixel.g, pixel.r, 0}; });
    return {std::move(frame), source.client, source.geometry_generation, source.compositor_time_100ns,
            source.foreground};
}

bool renderer_same_window_sample(const RendererWindowFrame& first, const RendererWindowFrame& second) {
    return first.generation == second.generation && first.bounds == second.bounds
        && first.foreground == second.foreground && first.compositor_time <= second.compositor_time;
}

std::int64_t renderer_capture_fence(std::uint64_t prior_qpc) {
    return ignatius_wgc::qpc_to_100ns(prior_qpc, frequency());
}

RendererPixelObservation observe_archive_main_window(ignatius_wgc::WindowCapture& capture,
    const std::filesystem::path& path, std::uint64_t prior_qpc, Clock::time_point deadline,
    RendererWindowProof& proof) {
    using namespace renderer_pixels_detail;
    const auto fence = renderer_capture_fence(prior_qpc);
    std::optional<RendererWindowFrame> previous;
    std::vector<Pixel> previous_crop;
    Box previous_box{};
    while (Clock::now() < deadline) {
        auto frame = renderer_window_frame(capture, deadline, fence);
        const char* reason = nullptr;
        auto verified = measure(frame.pixels, 0, false, true, reason, false);
        if (!verified) {
            previous.reset();
            continue;
        }
        auto pixels = cropped_pixels(frame.pixels, verified->crop);
        if (previous && renderer_same_window_sample(*previous, frame)
            && same_crop(previous_box, verified->crop) && previous_crop == pixels) {
            write_crop(path, verified->crop, pixels, deadline);
            proof = {{previous->compositor_time, frame.compositor_time}, frame.foreground};
            verified->facts.stable_frames = 2;
            return verified->facts;
        }
        previous = std::move(frame);
        previous_box = verified->crop;
        previous_crop = std::move(pixels);
        capture.restart(deadline);
    }
    throw std::runtime_error("owned WGC window did not display the stable main sentinel");
}

void renderer_capture_archive_window(ignatius_wgc::WindowCapture& capture, const std::filesystem::path& path,
                                     std::uint64_t prior_qpc, Clock::time_point deadline,
                                     RendererArchiveReceipt& receipt) {
    using namespace renderer_pixels_detail;
    const auto fence = renderer_capture_fence(prior_qpc);
    std::optional<RendererWindowFrame> previous;
    while (Clock::now() < deadline) {
        auto frame = renderer_window_frame(capture, deadline, fence);
        const Box client{0, 0, frame.pixels.width, frame.pixels.height};
        const auto surface = count_color(frame.pixels, client, {0x19, 0x1c, 0x22});
        const auto focus = count_color(frame.pixels, client, {0x8a, 0xb4, 0xf8});
        const bool verified = surface >= 2048 && focus >= 24 && find_anchor(frame.pixels).count == 0
            && count_color(frame.pixels, client, sentinel_color) == 0;
        if (!verified) {
            previous.reset();
            continue;
        }
        if (previous && renderer_same_window_sample(*previous, frame)) {
            write_crop(path, client, cropped_pixels(frame.pixels, client), deadline);
            receipt.active_width = frame.pixels.width;
            receipt.active_height = frame.pixels.height;
            receipt.active_surface_pixels = surface;
            receipt.active_focus_pixels = focus;
            receipt.wgc_active = {{previous->compositor_time, frame.compositor_time}, frame.foreground};
            return;
        }
        previous = std::move(frame);
        capture.restart(deadline);
    }
    throw std::runtime_error("owned WGC window did not display positive Material alternate frames");
}
#endif

void emit_renderer_archive(const RendererArchiveReceipt& receipt, bool window_capture) {
    const auto truth = [](bool value) { return value ? "true" : "false"; };
    std::cout << ",\"archived_client\":{\"pid\":" << receipt.pid
              << ",\"owned\":" << truth(receipt.child_owned)
              << ",\"process_machine\":" << receipt.process_machine << ",\"native_machine\":" << receipt.native_machine
              << ",\"machine_type_fallback\":" << truth(receipt.machine_type_fallback)
              << ",\"acquired_input\":" << receipt.acquired_input << ",\"acquired_output\":" << receipt.acquired_output
              << ",\"native_raw_alternate_heading\":" << truth(receipt.raw_alternate)
              << ",\"active_width\":" << receipt.active_width << ",\"active_height\":" << receipt.active_height
              << ",\"active_surface_pixels\":" << receipt.active_surface_pixels
              << ",\"active_focus_pixels\":" << receipt.active_focus_pixels
              << ",\"startup_facts\":" << receipt.startup_facts
              << ",\"startup_last_input\":" << receipt.startup_last_input
              << ",\"startup_last_output\":" << receipt.startup_last_output
              << ",\"startup_early_exit\":" << receipt.startup_early_exit
              << ",\"acquired_qpc\":" << receipt.acquired_qpc << ",\"acquired_capture_qpc\":" << receipt.acquired_capture_qpc
              << ",\"exit_waited\":" << truth(receipt.waited) << ",\"exit_code\":" << receipt.exit_code
              << ",\"restored_input\":" << receipt.restored_input << ",\"restored_output\":" << receipt.restored_output
              << ",\"restored_modes\":" << truth(receipt.restored_modes)
              << ",\"restored_native_sentinel\":" << truth(receipt.restored_sentinel)
              << ",\"restored_qpc\":" << receipt.restored_qpc << ",\"restored_capture_qpc\":" << receipt.restored_capture_qpc
              << ",\"restored_cursor_pixels\":"
              << (window_capture ? "null" : std::to_string(receipt.restored_pixels.cursor_pixels))
              << ",\"restored_cursor_area\":"
              << (window_capture ? "null" : std::to_string(receipt.restored_pixels.cursor_area))
              << ",\"restored_sentinel_pixels\":" << receipt.restored_pixels.sentinel_pixels
              << ",\"restored_stable_frames\":" << receipt.restored_pixels.stable_frames;
    if (window_capture) {
        std::cout << ",\"window_capture\":{\"software_d3d\":" << truth(receipt.wgc_software_device);
        const auto sample = [&](const char* phase, const RendererWindowProof& proof) {
            std::cout << ",\"" << phase << "\":{\"compositor_100ns\":[" << proof.compositor_times[0]
                      << ',' << proof.compositor_times[1] << "],\"foreground\":"
                      << (proof.compositor_times[1] ? truth(proof.foreground) : "null") << '}';
        };
        sample("baseline", receipt.wgc_baseline);
        sample("active", receipt.wgc_active);
        sample("restored", receipt.wgc_restored);
        std::cout << '}';
    }
    std::cout << '}';
}
