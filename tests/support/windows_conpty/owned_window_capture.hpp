#pragma once

#include <algorithm>
#include <array>
#include <atomic>
#include <chrono>
#include <cstdint>
#include <cstring>
#include <functional>
#include <limits>
#include <optional>
#include <stdexcept>
#include <string>
#include <thread>
#include <utility>
#include <vector>

namespace ignatius_wgc {
using Clock = std::chrono::steady_clock;

struct Size {
    int width{}, height{};
};

struct Rectangle {
    int left{}, top{}, right{}, bottom{};

    std::int64_t width() const { return std::int64_t{right} - left; }
    std::int64_t height() const { return std::int64_t{bottom} - top; }
};

inline bool operator==(Rectangle a, Rectangle b) {
    return a.left == b.left && a.top == b.top && a.right == b.right && a.bottom == b.bottom;
}

inline bool capture_size(Size size) {
    return size.width > 0 && size.width <= 1600 && size.height > 0 && size.height <= 1200;
}

// WGC returns a top-level window texture. Accept a client mapping only when
// its size and screen-coordinate origin match one unambiguous geometry.
inline std::optional<Rectangle> client_region(Size content, Rectangle client,
                                              Rectangle extended, Rectangle raw) {
    if (!capture_size(content) || client.width() <= 0 || client.height() <= 0) {
        return std::nullopt;
    }
    std::optional<Rectangle> result;
    for (const auto reference : {extended, raw, client}) {
        if (reference.width() != content.width || reference.height() != content.height
            || client.left < reference.left || client.top < reference.top
            || client.right > reference.right || client.bottom > reference.bottom) {
            continue;
        }
        const Rectangle candidate{client.left - reference.left, client.top - reference.top,
                                  client.right - reference.left, client.bottom - reference.top};
        if (result && !(*result == candidate)) {
            return std::nullopt;
        }
        result = candidate;
    }
    return result;
}

// Windows Graphics Capture exposes compositor QPC instants as 100 ns TimeSpan ticks.
inline std::int64_t qpc_to_100ns(std::uint64_t ticks, std::uint64_t frequency) {
    constexpr std::uint64_t units = 10000000;
    constexpr auto limit = static_cast<std::uint64_t>((std::numeric_limits<std::int64_t>::max)());
    if (frequency == 0 || frequency > limit / units || ticks / frequency > limit / units) {
        throw std::runtime_error("WGC QPC conversion outside integer bounds");
    }
    const auto whole = ticks / frequency * units;
    const auto fraction = ticks % frequency * units / frequency;
    if (fraction > limit - whole) {
        throw std::runtime_error("WGC QPC value overflow");
    }
    return static_cast<std::int64_t>(whole + fraction);
}

struct Pixel {
    std::uint8_t b, g, r, a;
};
static_assert(sizeof(Pixel) == 4, "BGRA pixel layout");

struct Frame {
    int width{}, height{};
    std::uint32_t geometry_generation{};
    std::int64_t compositor_time_100ns{};
    bool foreground{};
    Rectangle client;
    std::vector<Pixel> pixels;
};
} // namespace ignatius_wgc

#ifdef _WIN32
// Include this header at global scope, in a desktop TU that defines NOMINMAX.
// Use the installed Windows SDK cppwinrt include directory and link
// WindowsApp.lib, D3D11.lib, Dwmapi.lib, and User32.lib.
#include <windows.h>
#include <d3d11.h>
#include <d3d11_4.h>
#include <dwmapi.h>
#include <windows.graphics.capture.interop.h>
#include <windows.graphics.directx.direct3d11.interop.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Graphics.h>
#include <winrt/Windows.Graphics.Capture.h>
#include <winrt/Windows.Graphics.DirectX.h>
#include <winrt/Windows.Graphics.DirectX.Direct3D11.h>

namespace ignatius_wgc {
namespace wgc = winrt::Windows::Graphics::Capture;
namespace dx = winrt::Windows::Graphics::DirectX;
using WinSize = winrt::Windows::Graphics::SizeInt32;
constexpr auto pixel_format = dx::DirectXPixelFormat::B8G8R8A8UIntNormalized;

[[noreturn]] inline void capture_error(const char* stage, long code) {
    throw std::runtime_error(std::string(stage) + " hresult=" + std::to_string(code));
}

inline void check(HRESULT result, const char* stage) {
    if (FAILED(result)) capture_error(stage, result);
}

class Apartment {
public:
    Apartment() { winrt::init_apartment(winrt::apartment_type::multi_threaded); }
    ~Apartment() { winrt::uninit_apartment(); }
    Apartment(const Apartment&) = delete;
    Apartment& operator=(const Apartment&) = delete;
};

class DpiContext {
    const DPI_AWARENESS_CONTEXT prior_ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

public:
    DpiContext() {
        if (!prior_) capture_error("WGC enter physical DPI scope", HRESULT_FROM_WIN32(GetLastError()));
    }
    ~DpiContext() { SetThreadDpiAwarenessContext(prior_); }
    DpiContext(const DpiContext&) = delete;
    DpiContext& operator=(const DpiContext&) = delete;
};

struct Geometry {
    Rectangle client, extended, raw;
    bool foreground{};
};

inline bool operator==(const Geometry& a, const Geometry& b) {
    return a.client == b.client && a.extended == b.extended && a.raw == b.raw && a.foreground == b.foreground;
}

inline Rectangle physical(const RECT& rectangle) {
    return {static_cast<int>(rectangle.left), static_cast<int>(rectangle.top),
            static_cast<int>(rectangle.right), static_cast<int>(rectangle.bottom)};
}

inline Geometry current_geometry(HWND window) {
    const DpiContext physical_coordinates;
    RECT raw{}, extended{}, client{};
    if (!GetWindowRect(window, &raw) || !GetClientRect(window, &client)) {
        capture_error("WGC owned window geometry", HRESULT_FROM_WIN32(GetLastError()));
    }
    check(DwmGetWindowAttribute(window, DWMWA_EXTENDED_FRAME_BOUNDS, &extended,
                               static_cast<DWORD>(sizeof(extended))), "WGC DWM geometry");
    POINT start{client.left, client.top}, end{client.right, client.bottom};
    if (!ClientToScreen(window, &start) || !ClientToScreen(window, &end)) {
        capture_error("WGC physical client coordinates", HRESULT_FROM_WIN32(GetLastError()));
    }
    return {physical(RECT{start.x, start.y, end.x, end.y}), physical(extended), physical(raw),
            GetForegroundWindow() == window};
}

// The caller supplies the stable process handle and authenticates the exact
// Job, executable, title and HWND. Keep that handle and callback alive on this
// thread for the session. The sampler never selects a monitor or changes focus.
class WindowCapture {
    Apartment apartment_;
    HWND window_;
    HANDLE process_;
    std::function<void()> verify_owner_;
    const std::atomic_bool* cancellation_;
    winrt::com_ptr<ID3D11Device> device_;
    winrt::com_ptr<ID3D11DeviceContext> context_;
    winrt::com_ptr<ID3D11Texture2D> staging_;
    dx::Direct3D11::IDirect3DDevice direct3d_{nullptr};
    wgc::GraphicsCaptureItem item_{nullptr};
    wgc::Direct3D11CaptureFramePool pool_{nullptr};
    wgc::GraphicsCaptureSession session_{nullptr};
    WinSize size_{};
    Size staging_size_{};
    std::uint32_t generation_{};
    std::int64_t last_frame_{};
    bool warp_{};

    void check_deadline(Clock::time_point deadline) const {
        if (cancellation_ && cancellation_->load(std::memory_order_relaxed)) {
            throw std::runtime_error("WGC cancelled");
        }
        if (Clock::now() >= deadline) throw std::runtime_error("WGC deadline expired");
    }

    void check_target(Clock::time_point deadline) const {
        check_deadline(deadline);
        verify_owner_();
        DWORD current_pid{};
        const auto pid = GetProcessId(process_);
        if (!pid || WaitForSingleObject(process_, 0) != WAIT_TIMEOUT
            || GetWindowThreadProcessId(window_, &current_pid) == 0 || current_pid != pid
            || GetAncestor(window_, GA_ROOT) != window_ || !IsWindowVisible(window_) || IsIconic(window_)) {
            throw std::runtime_error("WGC target no longer matches the owned top-level window");
        }
        DWORD cloaked{};
        check(DwmGetWindowAttribute(window_, DWMWA_CLOAKED, &cloaked, static_cast<DWORD>(sizeof(cloaked))),
              "WGC window compositor visibility");
        if (cloaked != 0) throw std::runtime_error("WGC target is cloaked");
    }

    void pause(Clock::time_point deadline) const {
        check_deadline(deadline);
        std::this_thread::sleep_until((std::min)(deadline, Clock::now() + std::chrono::milliseconds{10}));
    }

    void create_device() {
        auto open = [this](D3D_DRIVER_TYPE driver) {
            device_ = nullptr;
            context_ = nullptr;
            return D3D11CreateDevice(nullptr, driver, nullptr, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                                     nullptr, 0, D3D11_SDK_VERSION, device_.put(), nullptr, context_.put());
        };
        auto result = open(D3D_DRIVER_TYPE_HARDWARE);
        if (result == DXGI_ERROR_UNSUPPORTED) {
            result = open(D3D_DRIVER_TYPE_WARP);
            warp_ = SUCCEEDED(result);
        }
        check(result, "WGC create BGRA D3D11 device");
        context_.as<ID3D11Multithread>()->SetMultithreadProtected(TRUE);
        winrt::com_ptr<::IInspectable> inspectable;
        check(CreateDirect3D11DeviceFromDXGIDevice(device_.as<IDXGIDevice>().get(), inspectable.put()),
              "WGC bridge D3D11 device");
        direct3d_ = inspectable.as<dx::Direct3D11::IDirect3DDevice>();
    }

    void create_staging(Size size) {
        if (staging_ && staging_size_.width == size.width && staging_size_.height == size.height) return;
        D3D11_TEXTURE2D_DESC descriptor{};
        descriptor.Width = static_cast<UINT>(size.width);
        descriptor.Height = static_cast<UINT>(size.height);
        descriptor.MipLevels = descriptor.ArraySize = 1;
        descriptor.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
        descriptor.SampleDesc.Count = 1;
        descriptor.Usage = D3D11_USAGE_STAGING;
        descriptor.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
        staging_ = nullptr;
        check(device_->CreateTexture2D(&descriptor, nullptr, staging_.put()), "WGC bounded staging texture");
        staging_size_ = size;
    }

    Frame copy(const wgc::Direct3D11CaptureFrame& frame, Rectangle region, const Geometry& geometry,
               std::int64_t rendered_at, Clock::time_point deadline) {
        const Size size{static_cast<int>(region.width()), static_cast<int>(region.height())};
        create_staging(size);
        using DxgiAccess = ::Windows::Graphics::DirectX::Direct3D11::IDirect3DDxgiInterfaceAccess;
        const auto access = frame.Surface().as<DxgiAccess>();
        winrt::com_ptr<ID3D11Texture2D> texture;
        check(access->GetInterface(__uuidof(ID3D11Texture2D), texture.put_void()), "WGC frame DXGI texture");
        D3D11_TEXTURE2D_DESC descriptor{};
        texture->GetDesc(&descriptor);
        if (descriptor.Format != DXGI_FORMAT_B8G8R8A8_UNORM || descriptor.SampleDesc.Count != 1
            || descriptor.Width < static_cast<UINT>(region.right)
            || descriptor.Height < static_cast<UINT>(region.bottom)) {
            throw std::runtime_error("WGC frame did not contain the validated BGRA client region");
        }
        const D3D11_BOX box{static_cast<UINT>(region.left), static_cast<UINT>(region.top), 0,
                            static_cast<UINT>(region.right), static_cast<UINT>(region.bottom), 1};
        context_->CopySubresourceRegion(staging_.get(), 0, 0, 0, 0, texture.get(), 0, &box);
        context_->Flush();
        const auto width = static_cast<std::size_t>(size.width);
        const auto height = static_cast<std::size_t>(size.height);
        Frame result{size.width, size.height, generation_, rendered_at, geometry.foreground, geometry.client,
                     std::vector<Pixel>(width * height)};
        D3D11_MAPPED_SUBRESOURCE mapped{};
        for (;;) {
            check_deadline(deadline);
            const auto status = context_->Map(staging_.get(), 0, D3D11_MAP_READ,
                                              D3D11_MAP_FLAG_DO_NOT_WAIT, &mapped);
            if (status != DXGI_ERROR_WAS_STILL_DRAWING) {
                check(status, "WGC map BGRA client");
                break;
            }
            pause(deadline);
        }
        const auto stride = width * sizeof(Pixel);
        if (!mapped.pData || static_cast<std::size_t>(mapped.RowPitch) < stride) {
            context_->Unmap(staging_.get(), 0);
            throw std::runtime_error("WGC frame row stride is invalid");
        }
        const auto* source = static_cast<const std::uint8_t*>(mapped.pData);
        for (std::size_t row = 0; row < height; ++row) {
            std::memcpy(result.pixels.data() + row * width, source + row * mapped.RowPitch, stride);
        }
        context_->Unmap(staging_.get(), 0);
        return result;
    }

public:
    WindowCapture(HWND window, HANDLE stable_process, std::function<void()> verify_owner,
                  Clock::time_point deadline, const std::atomic_bool* cancellation = nullptr) try
        : window_(window), process_(stable_process), verify_owner_(std::move(verify_owner)),
          cancellation_(cancellation) {
        if (!verify_owner_) throw std::runtime_error("WGC requires the caller's owned-window verifier");
        check_target(deadline);
        if (!wgc::GraphicsCaptureSession::IsSupported()) {
            throw std::runtime_error("WGC is unavailable on this Windows device or session");
        }
        create_device();
        const auto interop = winrt::get_activation_factory<wgc::GraphicsCaptureItem, IGraphicsCaptureItemInterop>();
        check(interop->CreateForWindow(window_, winrt::guid_of<wgc::IGraphicsCaptureItem>(), winrt::put_abi(item_)),
              "WGC create item for exact owned HWND");
        restart(deadline);
    } catch (const winrt::hresult_error& error) {
        capture_error("WGC initialize exact owned window", static_cast<long>(error.code().value));
    }

    ~WindowCapture() noexcept {
        try { close(); } catch (...) {}
    }
    WindowCapture(const WindowCapture&) = delete;
    WindowCapture& operator=(const WindowCapture&) = delete;

    bool uses_software_device() const noexcept { return warp_; }

    // A new session can deliver a snapshot of an unchanged window even when
    // a continuous session has no second paint to report. Keep the original
    // compositor timestamp so callers can distinguish those observations.
    void restart(Clock::time_point deadline) try {
        check_target(deadline);
        close();
        const auto size = item_.Size();
        if (!capture_size({size.Width, size.Height})) throw std::runtime_error("WGC initial buffer exceeds bounds");
        if (size_.Width != 0 && (size.Width != size_.Width || size.Height != size_.Height) && ++generation_ > 4) {
            throw std::runtime_error("WGC target resized too often");
        }
        size_ = size;
        last_frame_ = 0;
        pool_ = wgc::Direct3D11CaptureFramePool::CreateFreeThreaded(direct3d_, pixel_format, 2, size_);
        session_ = pool_.CreateCaptureSession(item_);
        // Suppress only the OS pointer overlay. Leave the system capture border in place.
        session_.IsCursorCaptureEnabled(false);
        check_target(deadline);
        session_.StartCapture();
    } catch (const winrt::hresult_error& error) {
        capture_error("WGC open capture session", static_cast<long>(error.code().value));
    }

    Frame next(Clock::time_point deadline, std::int64_t not_before_100ns = 0) try {
        if (!session_) throw std::runtime_error("WGC session already closed");
        for (;;) {
            check_target(deadline);
            const auto before = current_geometry(window_);
            auto frame = pool_.TryGetNextFrame();
            if (!frame) {
                pause(deadline);
                continue;
            }
            const auto content = frame.ContentSize();
            if (!capture_size({content.Width, content.Height})) {
                throw std::runtime_error("WGC resized buffer exceeds bounds");
            }
            if (content.Width != size_.Width || content.Height != size_.Height) {
                frame.Close();
                if (++generation_ > 4) throw std::runtime_error("WGC target resized too often");
                size_ = content;
                pool_.Recreate(direct3d_, pixel_format, 2, size_);
                continue;
            }
            const auto rendered_at = frame.SystemRelativeTime().count();
            const auto region = client_region({content.Width, content.Height},
                                              before.client, before.extended, before.raw);
            if (rendered_at <= (std::max)(last_frame_, not_before_100ns) || !region) {
                frame.Close();
                pause(deadline);
                continue;
            }
            auto result = copy(frame, *region, before, rendered_at, deadline);
            frame.Close();
            check_target(deadline);
            if (before == current_geometry(window_)) {
                last_frame_ = rendered_at;
                return result;
            }
            pause(deadline);
        }
    } catch (const winrt::hresult_error& error) {
        capture_error("WGC sample exact owned window", static_cast<long>(error.code().value));
    }

    void close() {
        auto session = std::exchange(session_, nullptr);
        auto pool = std::exchange(pool_, nullptr);
        try {
            if (session) session.Close();
        } catch (const winrt::hresult_error& error) {
            try { if (pool) pool.Close(); } catch (...) {}
            capture_error("WGC close capture session", static_cast<long>(error.code().value));
        }
        try {
            if (pool) pool.Close();
        } catch (const winrt::hresult_error& error) {
            capture_error("WGC close capture pool", static_cast<long>(error.code().value));
        }
    }
};
} // namespace ignatius_wgc
#endif
