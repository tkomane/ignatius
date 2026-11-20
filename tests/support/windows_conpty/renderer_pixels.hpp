#pragma once

// Include after the W07 Windows and standard headers and its Clock alias.
// This sensor consumes the supervisor's already authenticated, owned HWND.
#ifdef _MSC_VER
#pragma comment(lib, "Gdi32.lib")
#pragma comment(lib, "User32.lib")
#endif

struct RendererRgb {
    unsigned char r, g, b;
};

inline constexpr std::array<RendererRgb, 6> renderer_phase_colors{{
    {232, 88, 37}, {35, 154, 240}, {240, 202, 60},
    {55, 194, 204}, {223, 81, 163}, {179, 217, 62},
}};

struct RendererPixelObservation {
    int client_width{}, client_height{};
    int cell_width{}, cell_height{};
    int crop_width{}, crop_height{};
    std::size_t anchor_pixels{}, phase_pixels{}, sentinel_pixels{};
    std::size_t cursor_pixels{}, cursor_area{};
    unsigned stable_frames{};
};

namespace renderer_pixels_detail {
constexpr RendererRgb anchor_color{16, 220, 144};
constexpr RendererRgb sentinel_color{125, 32, 220};
constexpr RendererRgb cursor_color{255, 0, 255};
constexpr RendererRgb background_color{16, 16, 16};

struct Pixel {
    unsigned char b, g, r, unused;
    bool operator==(const Pixel& other) const noexcept {
        return b == other.b && g == other.g && r == other.r;
    }
};
static_assert(sizeof(Pixel) == 4, "32-bit DIB pixel layout");

struct Box {
    int left, top, right, bottom;
    int width() const { return right - left; }
    int height() const { return bottom - top; }
    std::size_t area() const {
        return static_cast<std::size_t>(width()) * static_cast<std::size_t>(height());
    }
};

struct Frame {
    int width, height;
    std::vector<Pixel> pixels;
    const Pixel* row(int y) const {
        return pixels.data() + static_cast<std::size_t>(y) * static_cast<std::size_t>(width);
    }
};

struct Measurement {
    RendererPixelObservation facts;
    Box crop;
};

[[noreturn]] inline void api_error(const char* operation, DWORD error) {
    throw std::runtime_error(std::string(operation) + " native_error=" + std::to_string(error));
}

inline bool same_rectangle(const RECT& left, const RECT& right) {
    return left.left == right.left && left.top == right.top &&
        left.right == right.right && left.bottom == right.bottom;
}

class PixelCoordinates {
    DPI_AWARENESS_CONTEXT previous_{};
public:
    PixelCoordinates() {
        previous_ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        if (!previous_) api_error("renderer SetThreadDpiAwarenessContext", GetLastError());
    }
    PixelCoordinates(const PixelCoordinates&) = delete;
    PixelCoordinates& operator=(const PixelCoordinates&) = delete;
    ~PixelCoordinates() {
        if (previous_) SetThreadDpiAwarenessContext(previous_);
    }
    void restore() {
        if (!SetThreadDpiAwarenessContext(previous_))
            api_error("renderer restore DPI context", GetLastError());
        previous_ = nullptr;
    }
};

inline void require_foreground(HWND window) {
    if (!IsWindow(window) || !IsWindowVisible(window) || IsIconic(window) ||
        GetForegroundWindow() != window)
        throw std::runtime_error("renderer owned window is not visible and foreground");
}

inline RECT visible_client(HWND window) {
    require_foreground(window);
    RECT client{};
    if (!GetClientRect(window, &client)) api_error("renderer GetClientRect", GetLastError());
    POINT start{client.left, client.top};
    POINT end{client.right, client.bottom};
    if (!ClientToScreen(window, &start) || !ClientToScreen(window, &end))
        api_error("renderer ClientToScreen", GetLastError());
    const RECT region{start.x, start.y, end.x, end.y};
    const auto width = static_cast<std::int64_t>(region.right) - region.left;
    const auto height = static_cast<std::int64_t>(region.bottom) - region.top;
    if (width < 1 || width > 2560 || height < 1 || height > 1600)
        throw std::runtime_error("renderer client pixel bounds are outside the capture limit");

    const HMONITOR monitor = MonitorFromRect(&region, MONITOR_DEFAULTTONULL);
    if (!monitor) throw std::runtime_error("renderer client has no display");
    MONITORINFO info{};
    info.cbSize = static_cast<DWORD>(sizeof(info));
    if (!GetMonitorInfoW(monitor, &info)) api_error("renderer GetMonitorInfoW", GetLastError());
    if (region.left < info.rcMonitor.left || region.top < info.rcMonitor.top ||
        region.right > info.rcMonitor.right || region.bottom > info.rcMonitor.bottom)
        throw std::runtime_error("renderer client is clipped or crosses monitors");

    // Screen copies include any higher visible window. Treat intersecting
    // window bounds conservatively and stop if window enumeration races.
    unsigned checked = 0;
    for (HWND above = GetWindow(window, GW_HWNDPREV); above;
         above = GetWindow(above, GW_HWNDPREV)) {
        if (++checked > 512 || !IsWindow(above))
            throw std::runtime_error("renderer could not bound the window stacking order");
        if (!IsWindowVisible(above) || IsIconic(above)) continue;
        RECT bounds{}, overlap{};
        if (!GetWindowRect(above, &bounds)) api_error("renderer upper window bounds", GetLastError());
        if (IntersectRect(&overlap, &bounds, &region))
            throw std::runtime_error("renderer client is covered by another window");
    }
    require_foreground(window);
    return region;
}

struct GdiCopy {
    HDC screen{}, memory{};
    HBITMAP bitmap{};
    HGDIOBJ previous{};
    GdiCopy() = default;
    GdiCopy(const GdiCopy&) = delete;
    GdiCopy& operator=(const GdiCopy&) = delete;
    ~GdiCopy() {
        if (previous && previous != HGDI_ERROR) SelectObject(memory, previous);
        if (bitmap) DeleteObject(bitmap);
        if (memory) DeleteDC(memory);
        if (screen) ReleaseDC(nullptr, screen);
    }
};

inline BITMAPINFOHEADER bitmap_info(int width, int height) {
    BITMAPINFOHEADER header{};
    header.biSize = static_cast<DWORD>(sizeof(header));
    header.biWidth = static_cast<LONG>(width);
    header.biHeight = -static_cast<LONG>(height); // top-down, BGRA
    header.biPlanes = 1;
    header.biBitCount = 32;
    header.biCompression = BI_RGB;
    return header;
}

inline Frame capture(const RECT& region) {
    const int width = static_cast<int>(region.right - region.left);
    const int height = static_cast<int>(region.bottom - region.top);
    GdiCopy handles;
    handles.screen = GetDC(nullptr);
    if (!handles.screen) api_error("renderer screen DC", GetLastError());
    handles.memory = CreateCompatibleDC(handles.screen);
    if (!handles.memory) api_error("renderer memory DC", GetLastError());

    BITMAPINFO info{};
    info.bmiHeader = bitmap_info(width, height);
    void* bits = nullptr;
    handles.bitmap = CreateDIBSection(handles.screen, &info, DIB_RGB_COLORS, &bits, nullptr, 0);
    if (!handles.bitmap || !bits) api_error("renderer CreateDIBSection", GetLastError());
    handles.previous = SelectObject(handles.memory, handles.bitmap);
    if (!handles.previous || handles.previous == HGDI_ERROR)
        api_error("renderer SelectObject", GetLastError());
    if (!BitBlt(handles.memory, 0, 0, width, height, handles.screen,
                 static_cast<int>(region.left), static_cast<int>(region.top), SRCCOPY | CAPTUREBLT))
        api_error("renderer BitBlt", GetLastError());
    if (!GdiFlush()) api_error("renderer GdiFlush", GetLastError());

    Frame frame{width, height, std::vector<Pixel>(
        static_cast<std::size_t>(width) * static_cast<std::size_t>(height))};
    std::memcpy(frame.pixels.data(), bits, frame.pixels.size() * sizeof(Pixel));
    return frame;
}

inline bool near_channel(unsigned char observed, unsigned char wanted) {
    const int delta = static_cast<int>(observed) - static_cast<int>(wanted);
    return delta >= -3 && delta <= 3;
}

inline bool matches(const Pixel& pixel, RendererRgb color) {
    return near_channel(pixel.r, color.r) && near_channel(pixel.g, color.g) &&
        near_channel(pixel.b, color.b);
}

inline bool inside(const Box& rect, const Frame& frame) {
    return rect.left >= 0 && rect.top >= 0 && rect.right <= frame.width &&
        rect.bottom <= frame.height && rect.right > rect.left && rect.bottom > rect.top;
}

inline std::size_t count_color(const Frame& frame, const Box& area, RendererRgb color) {
    std::size_t count = 0;
    for (int y = area.top; y < area.bottom; ++y) {
        const Pixel* row = frame.row(y);
        for (int x = area.left; x < area.right; ++x)
            if (matches(row[x], color)) ++count;
    }
    return count;
}

inline bool coverage(std::size_t matches_count, std::size_t area, std::size_t percent = 98) {
    return matches_count * 100 >= area * percent;
}

struct AnchorMatch {
    Box bounds;
    std::size_t count{};
};

inline AnchorMatch find_anchor(const Frame& frame) {
    AnchorMatch anchor{{frame.width, frame.height, 0, 0}};
    for (int y = 0; y < frame.height; ++y) {
        const Pixel* row = frame.row(y);
        for (int x = 0; x < frame.width; ++x) {
            if (!matches(row[x], anchor_color)) continue;
            ++anchor.count;
            anchor.bounds.left = (std::min)(anchor.bounds.left, x);
            anchor.bounds.top = (std::min)(anchor.bounds.top, y);
            anchor.bounds.right = (std::max)(anchor.bounds.right, x + 1);
            anchor.bounds.bottom = (std::max)(anchor.bounds.bottom, y + 1);
        }
    }
    return anchor;
}

inline std::optional<Measurement> measure(const Frame& frame, std::size_t phase,
                                          bool visible, bool main_expected, const char*& reason,
                                          bool observe_cursor = true) {
    const auto anchor_match = find_anchor(frame);
    const Box& anchor = anchor_match.bounds;
    const auto anchor_pixels = anchor_match.count;
    reason = "anchor_missing";
    if (anchor_pixels == 0) return std::nullopt;
    reason = "anchor_nonrectangular";
    if (anchor.width() % 4 != 0 || !coverage(anchor_pixels, anchor.area()))
        return std::nullopt;

    const int cell_width = anchor.width() / 4;
    const int cell_height = anchor.height();
    reason = "cell_geometry_out_of_bounds";
    if (cell_width < 3 || cell_width > 36 || cell_height < 6 || cell_height > 60)
        return std::nullopt;

    // The four contiguous anchor cells start at one-based row 8, column 10.
    // Their full width gives the horizontal pitch and their height the row pitch.
    auto cells = [&](int row, int column, int columns) {
        const int left = anchor.left + (column - 10) * cell_width;
        const int top = anchor.top + (row - 8) * cell_height;
        return Box{left, top, left + columns * cell_width, top + cell_height};
    };
    const Box phase_box = cells(2, 2, 6);
    const Box sentinel_box = cells(4, 2, 6);
    const Box cursor_box = cells(8, 20, 1);
    const Box crop{phase_box.left, phase_box.top, cursor_box.right, cursor_box.bottom};
    if (cells(1, 1, 1).left < 0 || cells(1, 1, 1).top < 0 ||
        !inside(phase_box, frame) || !inside(sentinel_box, frame) ||
        !inside(cursor_box, frame) || !inside(crop, frame))
        return std::nullopt;

    reason = "phase_tile_not_visible";
    const auto phase_pixels = count_color(frame, phase_box, renderer_phase_colors[phase]);
    if (!coverage(phase_pixels, phase_box.area())) return std::nullopt;

    reason = "main_screen_sentinel_mismatch";
    const auto sentinel_pixels = count_color(frame, sentinel_box, sentinel_color);
    if (main_expected) {
        if (!coverage(sentinel_pixels, sentinel_box.area())) return std::nullopt;
    } else if (sentinel_pixels != 0 ||
        !coverage(count_color(frame, sentinel_box, background_color), sentinel_box.area())) {
        return std::nullopt;
    }

    std::size_t cursor_pixels{}, cursor_area{};
    if (observe_cursor) {
        cursor_area = cursor_box.area();
        cursor_pixels = count_color(frame, cursor_box, cursor_color);
        if (visible) {
            reason = "expected_magenta_cursor_not_visible";
            if (!coverage(cursor_pixels, cursor_area, 90)) return std::nullopt;
        } else {
            reason = "expected_background_at_hidden_cursor";
            if (cursor_pixels != 0 ||
                !coverage(count_color(frame, cursor_box, background_color), cursor_area))
                return std::nullopt;
        }
    }

    return Measurement{{frame.width, frame.height, cell_width, cell_height,
        crop.width(), crop.height(), anchor_pixels, phase_pixels, sentinel_pixels,
        cursor_pixels, cursor_area, 0}, crop};
}

// Report aggregate color counts about the last owned client buffer only.
inline std::string capture_diagnostics(const Frame& frame, std::size_t phase) {
    if (frame.pixels.empty()) return " capture=unavailable";
    const auto [anchor, anchor_pixels] = find_anchor(frame);
    std::size_t phase_pixels = 0, sentinel_pixels = 0, magenta_pixels = 0;
    std::size_t black = 0, neutral = 0, background = 0;
    std::array<std::size_t, 64> chromatic_bins{};
    for (const auto& pixel : frame.pixels) {
        if (matches(pixel, renderer_phase_colors[phase])) ++phase_pixels;
        if (matches(pixel, sentinel_color)) ++sentinel_pixels;
        if (matches(pixel, cursor_color)) ++magenta_pixels;
        if (matches(pixel, background_color)) ++background;

        const int high = (std::max)({pixel.r, pixel.g, pixel.b});
        const int low = (std::min)({pixel.r, pixel.g, pixel.b});
        if (high <= 3) ++black;
        if (high - low <= 6) {
            ++neutral;
        } else {
            const auto bin = static_cast<std::size_t>(pixel.r >> 6) * 16
                + static_cast<std::size_t>(pixel.g >> 6) * 4
                + static_cast<std::size_t>(pixel.b >> 6);
            ++chromatic_bins[bin];
        }
    }

    std::ostringstream facts;
    facts << " capture=" << frame.width << 'x' << frame.height
          << " pixels=" << frame.pixels.size() << " anchor_near=" << anchor_pixels;
    if (anchor_pixels != 0) {
        facts << " anchor_bbox_ltrb=" << anchor.left << ',' << anchor.top
              << ',' << anchor.right << ',' << anchor.bottom;
    }
    facts << " phase_near=" << phase_pixels << " sentinel_near=" << sentinel_pixels
          << " magenta_near=" << magenta_pixels << " black=" << black
          << " neutral=" << neutral << " background_near=" << background
          << " chromatic=" << (frame.pixels.size() - neutral)
          << " top_chromatic_rgb_bin_lo_count=[";
    // Report at most three coarse bins. The R:G:B tuple is the lower
    // endpoint of each 64-value channel range, followed by the pixel count.
    for (int i = 0; i < 3; ++i) {
        auto largest = (std::max_element)(chromatic_bins.begin(), chromatic_bins.end());
        if (*largest == 0) break;
        const auto bin = static_cast<std::size_t>(largest - chromatic_bins.begin());
        if (i != 0) facts << ',';
        facts << ((bin >> 4) * 64) << ':' << (((bin >> 2) & 3) * 64)
              << ':' << ((bin & 3) * 64) << ':' << *largest;
        *largest = 0;
    }
    facts << ']';
    return facts.str();
}

inline std::vector<Pixel> cropped_pixels(const Frame& frame, const Box& crop) {
    std::vector<Pixel> result;
    result.reserve(crop.area());
    for (int y = crop.top; y < crop.bottom; ++y) {
        const Pixel* row = frame.row(y);
        for (int x = crop.left; x < crop.right; ++x)
            result.push_back(Pixel{row[x].b, row[x].g, row[x].r, 0});
    }
    return result;
}

inline void write_bytes(HANDLE file, const void* data, DWORD size) {
    DWORD written{};
    if (!WriteFile(file, data, size, &written, nullptr))
        api_error("renderer crop write", GetLastError());
    if (written != size) throw std::runtime_error("renderer crop write was incomplete");
}

inline void write_crop(const std::filesystem::path& path, const Box& crop,
                       const std::vector<Pixel>& pixels, Clock::time_point deadline) {
    if (Clock::now() >= deadline) throw std::runtime_error("renderer crop deadline");
    const DWORD bytes = static_cast<DWORD>(pixels.size() * sizeof(Pixel));
    auto image = bitmap_info(crop.width(), crop.height());
    image.biSizeImage = bytes;
    BITMAPFILEHEADER header{};
    header.bfType = 0x4d42;
    header.bfOffBits = static_cast<DWORD>(sizeof(header) + sizeof(image));
    header.bfSize = header.bfOffBits + bytes;

    const HANDLE file = CreateFileW(path.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_NEW,
                                    FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE) api_error("renderer crop create", GetLastError());
    try {
        write_bytes(file, &header, static_cast<DWORD>(sizeof(header)));
        write_bytes(file, &image, static_cast<DWORD>(sizeof(image)));
        write_bytes(file, pixels.data(), bytes);
        if (Clock::now() >= deadline) throw std::runtime_error("renderer crop deadline");
    } catch (...) {
        CloseHandle(file);
        DeleteFileW(path.c_str());
        throw;
    }
    if (!CloseHandle(file)) {
        const DWORD error = GetLastError();
        DeleteFileW(path.c_str());
        api_error("renderer crop close", error);
    }
}

struct StableSample {
    RECT client;
    Box crop;
    std::vector<Pixel> pixels;
};

inline bool same_crop(const Box& left, const Box& right) {
    return left.left == right.left && left.top == right.top &&
        left.right == right.right && left.bottom == right.bottom;
}
} // namespace renderer_pixels_detail

inline RendererPixelObservation observe_renderer_frame(
    HWND window, std::size_t phase, bool visible, bool main_expected,
    Clock::time_point deadline, const std::filesystem::path& crop) {
    using namespace renderer_pixels_detail;
    if (phase >= renderer_phase_colors.size() || !crop.is_absolute())
        throw std::runtime_error("renderer phase or crop path is outside the test contract");

    PixelCoordinates coordinates;
    std::optional<StableSample> previous;
    // Keep the last owned client capture in memory for the deadline summary.
    Frame frame{};
    const char* reason = "no_sample";
    constexpr std::chrono::milliseconds delay{45};

    while (Clock::now() < deadline) {
        const RECT before = visible_client(window);
        frame = capture(before);
        if (!same_rectangle(before, visible_client(window)))
            throw std::runtime_error("renderer client moved or resized during capture");
        auto verified = measure(frame, phase, visible, main_expected, reason);
        if (verified) {
            auto pixels = cropped_pixels(frame, verified->crop);
            if (previous && same_rectangle(previous->client, before) &&
                same_crop(previous->crop, verified->crop) && previous->pixels == pixels) {
                if (!same_rectangle(before, visible_client(window)))
                    throw std::runtime_error("renderer client changed before crop retention");
                coordinates.restore();
                write_crop(crop, verified->crop, pixels, deadline);
                verified->facts.stable_frames = 2;
                return verified->facts;
            }
            previous = StableSample{before, verified->crop, std::move(pixels)};
            reason = "no_two_equal_verified_frames";
        } else {
            previous.reset();
        }
        std::this_thread::sleep_until((std::min)(deadline, Clock::now() + delay));
    }
    throw std::runtime_error("renderer phase " + std::to_string(phase) + " deadline: "
                             + reason + capture_diagnostics(frame, phase));
}
