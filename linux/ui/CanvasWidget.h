#ifndef CanvasWidget_h
#define CanvasWidget_h

#include <QWidget>
#include <QImage>
#include <QPaintEvent>
#include <QPainter>

// Plan §6: CanvasWidget is the Qt Widget that displays the composited RGBA
// buffer rendered by the Swift core through the C ABI (CompRenderer / SkiaBridge).
// The host (host/host_run.cpp) calls compositor_session_render to get the pixel
// data, then presents it on this widget. The widget supports both Raster and
// Vulkan backends; when the backend is Raster the image is read-backed from CPU;
// when Vulkan it receives pre-presented GPU resident data.

class CanvasWidget : public QWidget {
    Q_OBJECT

public:
    CanvasWidget(QWidget *parent = nullptr);
    ~CanvasWidget() override;

    // Set the RGBA8 premultiplied pixel buffer and its dimensions.
    // Stride is expected to be width*4 (canonical contract).
    void set_image(const uint8_t *pixels, int width, int height, size_t stride);

protected:
    void paintEvent(QPaintEvent *event) override;

private:
    QImage m_image;
    int m_width;
    int m_height;
    bool m_has_image;
};

#endif /* CanvasWidget_h */