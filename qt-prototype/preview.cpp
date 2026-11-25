#include "preview.h"

#include <QImage>
#include <QLabel>
#include <QOpenGLContext>
#include <QOpenGLFunctions>
#include <QOpenGLWidget>
#include <QPainter>
#include <QStackedLayout>
#include <QSurfaceFormat>
#include <QTimer>
#include <QVariant>

#include <mpv/client.h>
#include <mpv/render.h>
#include <mpv/render_gl.h>

#include <algorithm>
#include <atomic>
#include <cmath>
#include <memory>
#include <optional>
#include <vector>

namespace {
QString mpvError(int code) { return QString::fromUtf8(mpv_error_string(code)); }

struct HandleDeleter {
    void operator()(mpv_handle *handle) const { mpv_terminate_destroy(handle); }
};

struct RenderDeleter {
    void operator()(mpv_render_context *context) const {
        mpv_render_context_set_update_callback(context, nullptr, nullptr);
        mpv_render_context_free(context);
    }
};
}

// Owns one mpv instance and its render context. All mpv calls below are on the
// GUI thread. The update callback only sets an atomic flag owned by this session.
class Surface : public QObject {
    Q_OBJECT
public:
    explicit Surface(QWidget *widget) : QObject(widget), widget_(widget) {
        timer_.setInterval(8);
        connect(&timer_, &QTimer::timeout, this, &Surface::poll);
    }

    QString initialize(mpv_render_param *parameters) {
        handle_.reset(mpv_create());
        if (!handle_)
            return tr("Could not create libmpv");
        const std::pair<const char *, const char *> options[] = {
            {"config", "no"}, {"terminal", "no"}, {"vo", "libmpv"}, {"profile", "fast"},
            {"hwdec", "no"}, {"idle", "yes"}, {"pause", "yes"},
            {"keep-open", "yes"}, {"load-scripts", "no"}, {"osc", "no"},
            {"input-default-bindings", "no"}, {"audio-display", "no"},
            {"audio-file-auto", "no"}, {"sub-auto", "no"}, {"osd-level", "0"}
        };
        for (const auto &[name, value] : options) {
            if (const int result = mpv_set_option_string(handle_.get(), name, value); result < 0)
                return mpvError(result);
        }
        if (qEnvironmentVariableIsSet("SPOOLCUT_QT_TEST_AUDIO_NULL"))
            mpv_set_option_string(handle_.get(), "ao", "null");
        if (const int result = mpv_initialize(handle_.get()); result < 0)
            return mpvError(result);

        mpv_render_context *context = nullptr;
        if (const int result = mpv_render_context_create(&context, handle_.get(), parameters); result < 0)
            return mpvError(result);
        render_.reset(context);
        mpv_render_context_set_update_callback(render_.get(), [](void *data) {
            static_cast<std::atomic_bool *>(data)->store(true, std::memory_order_release);
        }, &dirty_);
        mpv_observe_property(handle_.get(), 1, "time-pos", MPV_FORMAT_DOUBLE);
        mpv_observe_property(handle_.get(), 2, "eof-reached", MPV_FORMAT_FLAG);
        mpv_observe_property(handle_.get(), 3, "audio-out-params/channel-count", MPV_FORMAT_INT64);
        if (!path_.isEmpty())
            load();
        return {};
    }

    void shutdown() {
        timer_.stop();
        render_.reset();
        handle_.reset();
        loading_ = false;
        loaded_ = false;
    }

    void open(const QString &path, double seconds, bool playing) {
        path_ = path;
        requested_ = seconds;
        playing_ = playing;
        ++sourceVersion_;
        if (!loading_ && render_)
            load();
    }

    void seek(double seconds) { requested_ = seconds; }

    void setPlaying(bool playing) {
        playing_ = playing;
        if (loaded_) {
            int pause = !playing;
            mpv_set_property_async(handle_.get(), 0, "pause", MPV_FORMAT_FLAG, &pause);
        }
    }

    void stop() {
        path_.clear();
        requested_.reset();
        loaded_ = false;
        ++sourceVersion_;
        if (render_)
            send(0, {"stop"});
    }

    int render(mpv_render_param *parameters) {
        if (!render_)
            return MPV_ERROR_UNINITIALIZED;
        if (dirty_.exchange(false, std::memory_order_acq_rel))
            mpv_render_context_update(render_.get());
        return mpv_render_context_render(render_.get(), parameters);
    }

    bool ready() const { return render_ != nullptr; }
    bool hasMedia() const { return !path_.isEmpty(); }
    bool hasLoadedFrame() const { return loaded_; }

signals:
    void positionChanged(double seconds);
    void frameDrawn();
    void audioChanged(const QString &parameters);
    void mediaFailed(const QString &reason);
    void rendererFailed(const QString &reason);
    void rendererReady(const QString &renderer);
    void ended();

private:
    void send(uint64_t id, std::initializer_list<const char *> args) {
        std::vector<const char *> command(args);
        command.push_back(nullptr);
        if (const int result = mpv_command_async(handle_.get(), id, command.data()); result < 0) {
            if (id == 1)
                finishLoad(mpvError(result));
            else
                emit mediaFailed(mpvError(result));
        }
    }

    void load() {
        loading_ = true;
        loaded_ = false;
        loadingVersion_ = sourceVersion_;
        activeEntry_.reset();
        timer_.start();
        // File paths are absolute and UTF-8, including on Windows. libmpv reads
        // its argument array directly, so no command-line quoting is involved.
        const QByteArray file = path_.toUtf8();
        send(1, {"loadfile", file.constData(), "replace"});
    }

    void finishLoad(const QString &error) {
        loading_ = false;
        if (loadingVersion_ != sourceVersion_) {
            if (!path_.isEmpty())
                load();
            return;
        }
        loaded_ = error.isEmpty();
        if (loaded_)
            setPlaying(playing_);
        else
            emit mediaFailed(error);
    }

    void poll() {
        while (true) {
            const mpv_event *event = mpv_wait_event(handle_.get(), 0);
            if (event->event_id == MPV_EVENT_NONE)
                break;
            if (event->event_id == MPV_EVENT_COMMAND_REPLY && event->error < 0) {
                if (event->reply_userdata == 1 && loading_)
                    finishLoad(mpvError(event->error));
            } else if (event->event_id == MPV_EVENT_START_FILE) {
                activeEntry_ = static_cast<const mpv_event_start_file *>(event->data)->playlist_entry_id;
            } else if (event->event_id == MPV_EVENT_FILE_LOADED && loading_) {
                finishLoad({});
            } else if (event->event_id == MPV_EVENT_END_FILE) {
                const auto &end = *static_cast<const mpv_event_end_file *>(event->data);
                if (end.playlist_entry_id != activeEntry_)
                    continue;
                if (loading_) {
                    finishLoad(end.error < 0 ? mpvError(end.error) : tr("Playback stopped before loading"));
                } else if (loaded_ && end.reason == MPV_END_FILE_REASON_ERROR) {
                    loaded_ = false;
                    emit mediaFailed(mpvError(end.error));
                }
            } else if (event->event_id == MPV_EVENT_PROPERTY_CHANGE && loaded_) {
                const auto &property = *static_cast<const mpv_event_property *>(event->data);
                if (event->reply_userdata == 1 && property.format == MPV_FORMAT_DOUBLE)
                    emit positionChanged(*static_cast<const double *>(property.data));
                if (event->reply_userdata == 2 && property.format == MPV_FORMAT_FLAG
                    && *static_cast<const int *>(property.data))
                    emit ended();
                if (event->reply_userdata == 3 && property.format == MPV_FORMAT_INT64)
                    emit audioChanged(QString::number(*static_cast<const int64_t *>(property.data)));
            }
        }
        if (loaded_ && requested_) {
            const QByteArray value = QByteArray::number(*requested_, 'f', 6);
            requested_.reset();
            send(2, {"seek", value.constData(), "absolute+exact"});
        }
        if (dirty_.load(std::memory_order_acquire))
            widget_->update();
        if (!loading_ && path_.isEmpty())
            timer_.stop();
    }

    QWidget *widget_;
    QTimer timer_;
    std::atomic_bool dirty_ = false;
    std::unique_ptr<mpv_handle, HandleDeleter> handle_;
    std::unique_ptr<mpv_render_context, RenderDeleter> render_;
    QString path_;
    std::optional<double> requested_;
    uint64_t sourceVersion_ = 0;
    uint64_t loadingVersion_ = 0;
    std::optional<int64_t> activeEntry_;
    bool playing_ = false;
    bool loaded_ = false;
    bool loading_ = false;
};

namespace {
class SoftwareWidget final : public QWidget {
public:
    explicit SoftwareWidget(QWidget *parent) : QWidget(parent), surface_(this) {}
    Surface *session() { return &surface_; }

    void initialize() {
        mpv_render_param args[] = {
            {MPV_RENDER_PARAM_API_TYPE, const_cast<char *>(MPV_RENDER_API_TYPE_SW)}, {}
        };
        const auto error = surface_.initialize(args);
        if (error.isEmpty())
            emit surface_.rendererReady("Software");
        else
            emit surface_.rendererFailed(error);
    }

protected:
    void paintEvent(QPaintEvent *) override {
        QPainter painter(this);
        painter.fillRect(rect(), Qt::black);
        if (!surface_.ready() || !surface_.hasMedia() || width() < 1 || height() < 1)
            return;

        // Keep the fallback responsive at high DPI and cap the buffer size.
        QSize size = (this->size() * devicePixelRatioF());
        if (size.width() > 1920 || size.height() > 1080)
            size.scale(1920, 1080, Qt::KeepAspectRatio);
        int dimensions[] = {size.width(), size.height()};
        size_t stride = (static_cast<size_t>(size.width()) * 4 + 63) & ~size_t{63};
        const size_t length = stride * static_cast<size_t>(size.height());
        pixels_.resize(length + 63);
        void *buffer = pixels_.data();
        size_t space = pixels_.size();
        void *aligned = std::align(64, length, buffer, space);
        mpv_render_param args[] = {
            {MPV_RENDER_PARAM_SW_SIZE, dimensions},
            {MPV_RENDER_PARAM_SW_FORMAT, const_cast<char *>("bgr0")},
            {MPV_RENDER_PARAM_SW_STRIDE, &stride},
            {MPV_RENDER_PARAM_SW_POINTER, aligned}, {}
        };
        const int result = surface_.render(args);
        if (result < 0) {
            emit surface_.rendererFailed(mpvError(result));
            return;
        }
        // bgr0 leaves the last byte undefined. Use an opaque RGB image view.
        QImage image(static_cast<uchar *>(aligned), size.width(), size.height(), stride,
                     QImage::Format_RGB32);
        painter.drawImage(rect(), image);
        if (surface_.hasLoadedFrame())
            emit surface_.frameDrawn();
    }

private:
    Surface surface_;
    std::vector<uchar> pixels_;
};

class OpenGlWidget final : public QOpenGLWidget {
public:
    explicit OpenGlWidget(QWidget *parent) : QOpenGLWidget(parent), surface_(this) {
        QSurfaceFormat format = this->format();
        format.setDepthBufferSize(0);
        format.setStencilBufferSize(0);
        format.setSamples(0);
        setFormat(format);
    }

    ~OpenGlWidget() override { cleanup(); }
    Surface *session() { return &surface_; }

protected:
    void initializeGL() override {
        contextConnection_ = connect(context(), &QOpenGLContext::aboutToBeDestroyed, this,
                                     &OpenGlWidget::cleanup, Qt::DirectConnection);
        mpv_opengl_init_params resolver{
            [](void *context, const char *name) -> void * {
                auto *qtContext = static_cast<QOpenGLContext *>(context);
                return reinterpret_cast<void *>(qtContext->getProcAddress(name));
            },
            context()
        };
        mpv_render_param args[] = {
            {MPV_RENDER_PARAM_API_TYPE, const_cast<char *>(MPV_RENDER_API_TYPE_OPENGL)},
            {MPV_RENDER_PARAM_OPENGL_INIT_PARAMS, &resolver}, {}
        };
        const auto error = surface_.initialize(args);
        if (!error.isEmpty()) {
            emit surface_.rendererFailed(error);
            return;
        }
        const auto *name = context()->functions()->glGetString(GL_RENDERER);
        const QString driver = name ? QString::fromLatin1(reinterpret_cast<const char *>(name))
                                    : tr("unknown driver");
        emit surface_.rendererReady(QStringLiteral("OpenGL (%1)").arg(driver));
    }

    void paintGL() override {
        if (!surface_.ready() || !surface_.hasMedia()) {
            auto *gl = context()->functions();
            gl->glClearColor(0, 0, 0, 1);
            gl->glClear(GL_COLOR_BUFFER_BIT);
            return;
        }
        // Qt binds the widget FBO and sets a viewport in device pixels.
        // Use its actual viewport so fractional DPI uses the same rounding as Qt.
        auto *gl = context()->functions();
        GLint viewport[4]{};
        gl->glGetIntegerv(GL_VIEWPORT, viewport);
        if (viewport[2] <= 0 || viewport[3] <= 0)
            return;
        const GLuint target = defaultFramebufferObject();
        mpv_opengl_fbo fbo{static_cast<int>(target), viewport[2], viewport[3], 0};
        int flip = 1;
        int block = 0;
        mpv_render_param args[] = {
            {MPV_RENDER_PARAM_OPENGL_FBO, &fbo}, {MPV_RENDER_PARAM_FLIP_Y, &flip},
            {MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME, &block}, {}
        };
        gl->glDisable(GL_SCISSOR_TEST);
        gl->glColorMask(GL_TRUE, GL_TRUE, GL_TRUE, GL_TRUE);
        const int result = surface_.render(args);
        // libmpv can leave a different FBO bound. Qt still needs its own FBO
        // to compose the widget and to read a framebuffer snapshot in tests.
        gl->glBindFramebuffer(GL_FRAMEBUFFER, target);
        gl->glViewport(viewport[0], viewport[1], viewport[2], viewport[3]);
        if (result < 0)
            emit surface_.rendererFailed(mpvError(result));
        else if (surface_.hasLoadedFrame())
            emit surface_.frameDrawn();
    }

private:
    void cleanup() {
        if (!contextConnection_)
            return;
        disconnect(contextConnection_);
        contextConnection_ = {};
        makeCurrent();
        surface_.shutdown();
        doneCurrent();
    }

    Surface surface_;
    QMetaObject::Connection contextConnection_;
};
}

Preview::Preview(QWidget *parent)
    : QWidget(parent), layout_(new QStackedLayout(this)), placeholder_(new QLabel(this)) {
    placeholder_->setAlignment(Qt::AlignCenter);
    placeholder_->setWordWrap(true);
    placeholder_->setText(tr("Open a video to preview it"));
    layout_->addWidget(placeholder_);
}

Preview::~Preview() {
    // Destroy the renderer while the native window can still supply its GL context.
    delete view_;
}

void Preview::installSurface(bool software) {
    delete view_;
    software_ = software;
    if (software) {
        auto *view = new SoftwareWidget(this);
        view_ = view;
        surface_ = view->session();
    } else {
        auto *view = new OpenGlWidget(this);
        view_ = view;
        surface_ = view->session();
    }
    connect(surface_, &Surface::rendererFailed, this, &Preview::fallback, Qt::QueuedConnection);
    connect(surface_, &Surface::rendererReady, this, &Preview::handleRenderer);
    connect(surface_, &Surface::mediaFailed, this, &Preview::handleFailure);
    connect(surface_, &Surface::frameDrawn, this, &Preview::frameDrawn);
    connect(surface_, &Surface::audioChanged, this, &Preview::audioChanged);
    connect(surface_, &Surface::ended, this, &Preview::ended);
    connect(surface_, &Surface::positionChanged, this, [this](double seconds) {
        position_ = seconds;
        emit positionChanged(seconds);
    });
    surface_->open(path_, position_, playing_);
    layout_->addWidget(view_);
    layout_->setCurrentWidget(view_);
    if (software) {
        static_cast<SoftwareWidget *>(view_)->initialize();
    } else {
        // Some QPA plugins never call initializeGL when they cannot create a context.
        QTimer::singleShot(1200, view_, [this] {
            if (renderer_.isEmpty())
                fallback(tr("Qt could not create an OpenGL widget context"));
        });
    }
}

void Preview::fallback(const QString &reason) {
    // Queued notifications may outlive a surface that already fell back.
    if (sender() && sender() != surface_)
        return;
    if (software_) {
        handleFailure(reason);
        return;
    }
    fallbackReason_ = reason;
    installSurface(true);
}

void Preview::handleRenderer(const QString &renderer) {
    renderer_ = renderer;
    setToolTip(fallbackReason_.isEmpty() ? renderer
                                        : tr("%1 — OpenGL fallback: %2").arg(renderer, fallbackReason_));
    emit rendererChanged(toolTip());
}

void Preview::handleFailure(const QString &reason) {
    placeholder_->setText(reason);
    layout_->setCurrentWidget(placeholder_);
    emit failed(reason);
}

QString Preview::renderer() const { return renderer_; }

void Preview::open(const QString &path, double seconds, bool playing) {
    path_ = path;
    position_ = seconds;
    playing_ = playing;
    if (!surface_) {
        installSurface(qEnvironmentVariable("SPOOLCUT_QT_PREVIEW") == QStringLiteral("software"));
    } else {
        layout_->setCurrentWidget(view_);
        surface_->open(path, seconds, playing);
    }
}

void Preview::seek(double seconds) {
    if (!surface_ || !std::isfinite(seconds))
        return;
    position_ = std::max(0.0, seconds);
    surface_->seek(position_);
}

void Preview::setPlaying(bool playing) {
    playing_ = playing;
    if (surface_)
        surface_->setPlaying(playing);
}

void Preview::stop() {
    playing_ = false;
    path_.clear();
    position_ = 0;
    if (surface_)
        surface_->stop();
    placeholder_->setText(tr("Open a video to preview it"));
    layout_->setCurrentWidget(placeholder_);
}

#include "preview.moc"
