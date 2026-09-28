#include "CanvasWidget.h"
#include <QPainter>
#include <QImage>

CanvasWidget::CanvasWidget(QWidget *parent)
    : QWidget(parent)
    , m_width(0)
    , m_height(0)
    , m_has_image(false)
{
    setMinimumSize(400, 300);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
}

CanvasWidget::~CanvasWidget() {
}

void CanvasWidget::set_image(const uint8_t *pixels, int width, int height, size_t /*stride*/) {
    if (!pixels || width <= 0 || height <= 0) {
        m_has_image = false;
        update();
        return;
    }

    // Build QImage from the premultiplied RGBA8 canonical buffer.
    // The canonical contract: stride == width*4, premultiplied RGBA8,
    // bottom-to-top vs top-to-bottom: QImage uses top-left origin.
    // The compositor's buffer is top-down; we need to flip it for QImage.
    m_width = width;
    m_height = height;
    m_has_image = true;

    // Allocate QImage with the right format. We use Format_RGBA8888 which
    // is premultiplied alpha; the source buffer is already premultiplied so
    // no extra conversion is needed except the vertical flip.
    QImage img(pixels, width, height, width * 4, QImage::Format_RGBA8888);

    // Flip vertically so QImage's (0,0) = top-left matches compositor's origin.
    m_image = img.scaled(width, height, Qt::IgnoreAspectRatio,
                         Qt::SmoothTransformation);
    // Actually let's just store and flip on paint:
    m_image = img;
    update();
}

void CanvasWidget::paintEvent(QPaintEvent * /*event*/) {
    if (!m_has_image || m_width <= 0 || m_height <= 0) {
        QWidget::paintEvent(nullptr);
        return;
    }

    QPainter painter(this);
    // Scale to widget size while keeping aspect ratio, with black bars.
    painter.drawImage(QRect(0, 0, width(), height()), m_image.scaled(width(), height(),
                                                                     Qt::KeepAspectRatioByExpanding,
                                                                     Qt::SmoothTransformation));
}