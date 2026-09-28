#include "CanvasWidget.h"
#include "CanonicalBuffer.h"

// The C kernels are pure C (no extern "C" guards yet — that C++-safety is ENG-2's
// Qt-host integration). Wrap the headers so the C++ host binds C linkage.
extern "C" {
#include "AdjustPixels.h"
#include "LensPixels.h"
#include "NoisePixels.h"
}

#include <QPainter>
#include <QPainterPath>

CanvasWidget::CanvasWidget(QWidget *parent) : QWidget(parent) {
    setMinimumSize(320, 240);
    setAttribute(Qt::WA_OpaquePaintEvent);
}

bool CanvasWidget::loadFile(const QString &path) {
    QImage loaded(path);
    if (loaded.isNull()) return false;
    m_width = loaded.width();
    m_height = loaded.height();
    m_canonical = compositor::canonical_from_qimage(loaded);
    syncDisplay();
    update();
    return true;
}

bool CanvasWidget::saveFile(const QString &path, const char *format) {
    if (m_display.isNull()) return false;
    return m_display.convertToFormat(QImage::Format_RGBA8888).save(path, format);
}

void CanvasWidget::syncDisplay() {
    if (m_width == 0 || m_height == 0) {
        m_display = QImage();
        return;
    }
    m_display = compositor::qimage_from_canonical(m_canonical.data(), m_width, m_height);
}

void CanvasWidget::paintEvent(QPaintEvent *) {
    QPainter p(this);
    p.fillRect(rect(), Qt::darkGray);
    if (m_display.isNull()) {
        p.setPen(Qt::white);
        p.drawText(rect(), Qt::AlignCenter, "No image — File > Open…");
        return;
    }
    // Fit the image inside the widget, preserving aspect.
    QSize s = m_display.size().scaled(width(), height(), Qt::KeepAspectRatio);
    QRect target((width() - s.width()) / 2, (height() - s.height()) / 2, s.width(), s.height());
    p.drawImage(target, m_display);
}

void CanvasWidget::applyNoise(float amount, int gaussian, int monochromatic, uint32_t seed) {
    if (m_width == 0) return;
    noise_add(m_canonical.data(), (size_t)m_width, (size_t)m_height,
              (size_t)m_width * 4u, amount, gaussian, monochromatic, seed);
    syncDisplay();
    update();
}

void CanvasWidget::applyGrain(double amount, double size, double roughness, uint32_t seed) {
    if (m_width == 0) return;
    adjust_grain(m_canonical.data(), (size_t)m_width, (size_t)m_height,
                (size_t)m_width * 4, amount, size, roughness, seed, 0.0, 0.0, 1.0);
    syncDisplay();
    update();
}

void CanvasWidget::applyGradientMap(const uint8_t table[256 * 3]) {
    if (m_width == 0) return;
    adjust_gradient_map(m_canonical.data(), (size_t)m_width, (size_t)m_height,
                       (size_t)m_width * 4, table);
    syncDisplay();
    update();
}

void CanvasWidget::applyLensDistort(double k) {
    if (m_width == 0) return;
    // lens_distort reads source, writes destination — needs a temp copy.
    std::vector<uint8_t> src = m_canonical;
    lens_distort(src.data(), m_canonical.data(), (size_t)m_width, (size_t)m_height,
                 (size_t)m_width * 4, k);
    syncDisplay();
    update();
}