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

#include "renderer_archive_shell.hpp"

void renderer_run_archive(const std::wstring& archive, const std::wstring& pipe,
                          const std::wstring& nonce, const std::wstring& work, HANDLE input,
                          const std::optional<Child>& shell) {
    const auto nonce_text = ascii(nonce);
    const auto deadline = Clock::now() + 20s;
    RendererLoopback loopback;
    Child client;
    if (shell) {
        client = renderer_start_archive_from_shell(work, shell->process.value, archive, deadline);
    } else {
        const auto endpoint = L"postgres://fixture@127.0.0.1:" + loopback.reserve()
            + L"/renderer_fixture?sslmode=disable&connect_timeout=2";
        STARTUPINFOW startup{};
        startup.cb = sizeof(startup);
        client = create(archive, {L"connect", L"--no-history", endpoint}, work, &startup, 0);
    }
    try {
        renderer_report_process(pipe, nonce, 'P', client);
        if (!shell) resume(client);

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
        if (shell) {
            const auto cell = renderer_workbench_query(client, input, deadline, work, pipe, nonce);
            renderer_shell_stage(pipe, nonce, 1, cell);
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
        if (shell) renderer_use_restored_shell(*shell, work, input, deadline, pipe, nonce);
    } catch (...) {
        // Terminate only the process created directly by this actor. The
        // supervisor owns cleanup of the PowerShell-launched Job.
        if (!shell && WaitForSingleObject(client.process.value, 0) == WAIT_TIMEOUT) {
            TerminateProcess(client.process.value, 125);
            WaitForSingleObject(client.process.value, 500);
        }
        throw;
    }
}

struct RendererWindowProof {
    std::array<std::int64_t, 2> compositor_times{};
    bool foreground{};
    int grid_left{}, grid_top{};
    int cell_width{}, cell_height{};
};

#ifdef IGNATIUS_WORKBENCH_GRID
struct RendererGridWitness {
    std::uint64_t filter_input_qpc{}, console_verified_qpc{};
    std::int64_t fence_100ns{}, compositor_100ns{};
    int width{}, height{};
};
#endif

struct RendererArchiveReceipt {
    DWORD pid{}, acquired_input{}, acquired_output{}, restored_input{}, restored_output{}, exit_code{STILL_ACTIVE};
    USHORT process_machine{}, native_machine{};
    bool machine_type_fallback{};
    std::uint64_t acquired_qpc{}, acquired_capture_qpc{}, restored_qpc{}, restored_capture_qpc{};
    bool child_owned{}, waited{}, raw_alternate{}, restored_sentinel{}, restored_modes{};
    bool workbench_expected{}, native_powershell{}, powershell_prompt{};
    std::optional<COORD> workbench_result;
    std::array<std::size_t, 2> workbench_ink{};
    int active_width{}, active_height{};
    std::size_t active_surface_pixels{}, active_focus_pixels{};
    DWORD startup_facts{}, startup_last_input{}, startup_last_output{}, startup_early_exit{STILL_ACTIVE};
    RendererPixelObservation restored_pixels{};
    bool wgc_software_device{};
    std::optional<std::size_t> wgc_active_cursor_pixels;
    RendererWindowProof wgc_baseline{}, wgc_active{}, wgc_restored{};
#ifdef IGNATIUS_WORKBENCH_GRID
    std::optional<RendererGridWitness> grid_witness;
#endif
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
            "authenticate the child executable in the exact renderer Job");
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

RendererPixelObservation observe_archive_window_phase(ignatius_wgc::WindowCapture& capture,
    const std::filesystem::path& path, std::uint64_t prior_qpc, Clock::time_point deadline,
    std::size_t phase, bool observe_cursor, RendererWindowProof& proof) {
    using namespace renderer_pixels_detail;
    const auto fence = renderer_capture_fence(prior_qpc);
    std::optional<RendererWindowFrame> previous;
    std::vector<Pixel> previous_crop;
    Box previous_box{};
    while (Clock::now() < deadline) {
        auto frame = renderer_window_frame(capture, deadline, fence);
        require(!observe_cursor || !frame.foreground, "window cursor calibration samples an inactive HWND");
        const char* reason = nullptr;
        auto verified = measure(frame.pixels, phase, renderer_visible[phase], renderer_main[phase], reason,
                                observe_cursor);
        if (!verified) {
            previous.reset();
            continue;
        }
        auto pixels = cropped_pixels(frame.pixels, verified->crop);
        if (previous && renderer_same_window_sample(*previous, frame)
            && same_crop(previous_box, verified->crop) && previous_crop == pixels) {
            write_crop(path, verified->crop, pixels, deadline);
            const auto anchor = find_anchor(frame.pixels).bounds;
            const auto width = verified->facts.cell_width;
            const auto height = verified->facts.cell_height;
            proof = {{previous->compositor_time, frame.compositor_time}, frame.foreground,
                     anchor.left - 9 * width, anchor.top - 7 * height, width, height};
            verified->facts.stable_frames = 2;
            return verified->facts;
        }
        previous = std::move(frame);
        previous_box = verified->crop;
        previous_crop = std::move(pixels);
        capture.restart(deadline);
    }
    throw std::runtime_error("owned WGC window did not display the required stable phase");
}

#ifdef IGNATIUS_WORKBENCH_GRID
RendererGridWitness renderer_capture_grid_witness(ignatius_wgc::WindowCapture& capture,
    const std::filesystem::path& path, const std::array<std::uint32_t, 11>& cells,
    std::uint64_t filter_input_qpc, std::uint64_t console_qpc, const RendererWindowProof& grid,
    Clock::time_point deadline) {
    using namespace renderer_pixels_detail;
    // The actor specifies the Result title, matching status, header, and first
    // data rows, then the status text, row_code and wide_a column positions.
    require(cells[0] >= 6 && cells[0] < cells[1] && cells[1] < cells[2] && cells[2] < cells[3]
            && cells[3] <= 24 && cells[4] >= 1 && cells[4] <= 59 && cells[5] >= 1
            && cells[5] < cells[6] && cells[5] <= 81 && cells[6] <= 81
            && grid.cell_width > 0 && grid.cell_height > 0, "bounded retained grid witness cells");
    const int title = static_cast<int>(cells[0]), status = static_cast<int>(cells[1]);
    const int header = static_cast<int>(cells[2]), first = static_cast<int>(cells[3]);
    const int match = static_cast<int>(cells[4]), code = static_cast<int>(cells[5]);
    const int wide = static_cast<int>(cells[6]);
    const auto box = [&](int x, int y, int columns, int rows = 1) {
        return Box{grid.grid_left + x * grid.cell_width, grid.grid_top + y * grid.cell_height,
            grid.grid_left + (x + columns) * grid.cell_width, grid.grid_top + (y + rows) * grid.cell_height};
    };
    const auto crop = box(0, title, 90, 26 - title);
    require(crop.area() <= 350000, "bounded Results-only WGC crop");
    const auto fence = renderer_capture_fence(filter_input_qpc);
    // Reserve time within the existing deadline to retain at most one
    // eligible Results-only crop if the pixel predicates never pass.
    const auto sample_deadline = deadline - 200ms;
    capture.restart(sample_deadline);
    std::optional<RendererWindowFrame> rejected;
    unsigned rejected_anchors{};
    while (Clock::now() < sample_deadline) {
        std::optional<RendererWindowFrame> sample;
        try {
            sample.emplace(renderer_window_frame(capture, sample_deadline, fence));
        } catch (const std::runtime_error& error) {
            if (std::string_view(error.what()) != "WGC deadline expired") throw;
            break;
        }
        auto& frame = *sample;
        require(!frame.foreground && inside(crop, frame.pixels), "retained grid stays inside the owned inactive window");
        const auto ink = [](const Pixel& pixel) { return pixel.r >= 100 && pixel.g >= 110 && pixel.b >= 100; };
        const auto ink_pixels = [&](const Box& area) {
            std::size_t total{};
            for (int y = area.top; y < area.bottom; ++y) {
                const auto* row = frame.pixels.row(y);
                total += static_cast<std::size_t>(std::count_if(row + area.left, row + area.right, ink));
            }
            return total;
        };
        const auto text_anchor = [&](int x, int y) {
            int painted{};
            for (int i = 0; i < 8; ++i) {
                if (ink_pixels(box(x + i, y, 1)) >= 2) ++painted;
            }
            return painted >= 6;
        };
        const auto wide_pair = [&](int y) {
            const auto a = box(wide, y, 2);
            const auto b = box(wide + 2, y, 2);
            std::size_t difference{};
            for (int row = a.top; row < a.bottom; ++row) {
                const auto* pixels = frame.pixels.row(row);
                for (int x = 0; x < a.width(); ++x) {
                    if (ink(pixels[a.left + x]) != ink(pixels[b.left + x])) ++difference;
                }
            }
            // Visible glyph strokes must differ. Identical missing-character
            // boxes or a flat background do not establish a wide visual cell.
            return ink_pixels(a) >= 4 && ink_pixels(b) >= 4 && difference >= 4;
        };
        const auto matching_digits = std::all_of(renderer_grid_filter_digits.begin(),
            renderer_grid_filter_digits.end(), [&](int digit) { return ink_pixels(box(match + digit, status, 1)) >= 4; });
        // Failed-frame mask bits: digits, matching text, header, wide row 1,
        // wide row 2, Material surface.
        const std::array<bool, 6> anchors{matching_digits, text_anchor(match, status), text_anchor(code, header),
            wide_pair(first), wide_pair(first + 1), count_color(frame.pixels, crop, {0x19, 0x1c, 0x22}) >= 512};
        unsigned mask{};
        for (std::size_t bit = 0; bit < anchors.size(); ++bit) {
            if (anchors[bit]) mask |= 1U << bit;
        }
        if (mask == 0x3f) {
            write_crop(path, crop, cropped_pixels(frame.pixels, crop), deadline);
            return {filter_input_qpc, console_qpc, fence, frame.compositor_time, crop.width(), crop.height()};
        }
        rejected = std::move(frame);
        rejected_anchors = mask;
    }
    const auto stamps = " input_qpc=" + std::to_string(filter_input_qpc)
        + " console_verified_qpc=" + std::to_string(console_qpc) + " fence_100ns=" + std::to_string(fence);
    if (!rejected) throw std::runtime_error("filtered grid received no WGC frame past the input fence" + stamps);
    write_crop(path.parent_path() / L"workbench-filtered-grid-diagnostic.bmp", crop,
        cropped_pixels(rejected->pixels, crop), deadline);
    throw std::runtime_error("filtered grid WGC frame failed anchor checks" + stamps
        + " frame_100ns=" + std::to_string(rejected->compositor_time)
        + " anchor_mask=" + std::to_string(rejected_anchors));
}
#endif

void renderer_capture_archive_window(ignatius_wgc::WindowCapture& capture, const std::filesystem::path& path,
                                     std::uint64_t prior_qpc, Clock::time_point deadline,
                                     bool observe_cursor, RendererArchiveReceipt& receipt) {
    using namespace renderer_pixels_detail;
    const auto fence = renderer_capture_fence(prior_qpc);
    std::optional<RendererWindowFrame> previous;
    while (Clock::now() < deadline) {
        auto frame = renderer_window_frame(capture, deadline, fence);
        require(!observe_cursor || !frame.foreground, "archive cursor measurement samples an inactive HWND");
        const Box client{0, 0, frame.pixels.width, frame.pixels.height};
        const auto surface = count_color(frame.pixels, client, {0x19, 0x1c, 0x22});
        const auto focus = count_color(frame.pixels, client, {0x8a, 0xb4, 0xf8});
        const auto cursor = observe_cursor ? count_color(frame.pixels, client, cursor_color) : 0;
        std::array<std::size_t, 2> result_ink{};
        if (receipt.workbench_result) {
            const auto& grid = receipt.wgc_baseline;
            const auto& location = *receipt.workbench_result;
            const int left = grid.grid_left + location.X * grid.cell_width;
            const int top = grid.grid_top + location.Y * grid.cell_height;
            for (std::size_t digit = 0; digit < result_ink.size(); ++digit) {
                const int x = left + static_cast<int>(digit) * grid.cell_width;
                const Box box{x, top, x + grid.cell_width, top + grid.cell_height};
                require(inside(box, frame.pixels), "the native result cells map inside the same WGC frame");
                for (int row = box.top; row < box.bottom; ++row) {
                    const auto* pixels = frame.pixels.row(row);
                    for (int column = box.left; column < box.right; ++column) {
                        const auto& pixel = pixels[column];
                        if (pixel.r >= 120 && pixel.g >= 120 && pixel.b >= 120) ++result_ink[digit];
                    }
                }
            }
        }
        const bool verified = surface >= 2048 && focus >= 24 && find_anchor(frame.pixels).count == 0
            && count_color(frame.pixels, client, sentinel_color) == 0 && cursor == 0
            && (!receipt.workbench_result || (result_ink[0] >= 4 && result_ink[1] >= 4));
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
            receipt.workbench_ink = result_ink;
            if (observe_cursor) receipt.wgc_active_cursor_pixels = cursor;
            receipt.wgc_active.compositor_times = {previous->compositor_time, frame.compositor_time};
            receipt.wgc_active.foreground = frame.foreground;
            return;
        }
        previous = std::move(frame);
        capture.restart(deadline);
    }
    throw std::runtime_error("owned WGC window did not display the required Material alternate frames");
}
#endif

void emit_renderer_window_proof(const RendererWindowProof& proof) {
    std::cout << "{\"compositor_100ns\":[" << proof.compositor_times[0] << ',' << proof.compositor_times[1]
              << "],\"foreground\":"
              << (!proof.compositor_times[1] ? "null" : proof.foreground ? "true" : "false") << '}';
}

void emit_renderer_archive(const RendererArchiveReceipt& receipt, bool window_capture, bool window_cursor_calibration) {
    const auto truth = [](bool value) { return value ? "true" : "false"; };
    const bool report_cursor = !window_capture || window_cursor_calibration;
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
              << (report_cursor ? std::to_string(receipt.restored_pixels.cursor_pixels) : "null")
              << ",\"restored_cursor_area\":"
              << (report_cursor ? std::to_string(receipt.restored_pixels.cursor_area) : "null")
              << ",\"restored_sentinel_pixels\":" << receipt.restored_pixels.sentinel_pixels
              << ",\"restored_stable_frames\":" << receipt.restored_pixels.stable_frames;
    if (receipt.workbench_expected) {
        std::cout << ",\"powershell_workbench\":{\"native_shell\":" << truth(receipt.native_powershell)
                  << ",\"result_rendered\":" << truth(receipt.workbench_result.has_value())
                  << ",\"result_ink_pixels\":[" << receipt.workbench_ink[0] << ',' << receipt.workbench_ink[1] << ']'
                  << ",\"prompt_executed\":" << truth(receipt.powershell_prompt) << '}';
    }
#ifdef IGNATIUS_WORKBENCH_GRID
    if (receipt.grid_witness) {
        const auto& witness = *receipt.grid_witness;
        std::cout << ",\"filtered_grid_witness\":{\"filter_input_qpc\":" << witness.filter_input_qpc
                  << ",\"console_verified_qpc\":" << witness.console_verified_qpc
                  << ",\"fence_100ns\":" << witness.fence_100ns << ",\"compositor_100ns\":" << witness.compositor_100ns
                  << ",\"width\":" << witness.width << ",\"height\":" << witness.height << '}';
    }
#endif
    if (window_cursor_calibration) {
        std::cout << ",\"active_cursor_pixels\":"
                  << (receipt.wgc_active_cursor_pixels ? std::to_string(*receipt.wgc_active_cursor_pixels) : "null");
    }
    if (window_capture) {
        std::cout << ",\"window_capture\":{\"software_d3d\":" << truth(receipt.wgc_software_device);
        const auto sample = [&](const char* phase, const RendererWindowProof& proof) {
            std::cout << ",\"" << phase << "\":";
            emit_renderer_window_proof(proof);
        };
        sample("baseline", receipt.wgc_baseline);
        sample("active", receipt.wgc_active);
        sample("restored", receipt.wgc_restored);
        std::cout << '}';
    }
    std::cout << '}';
}
