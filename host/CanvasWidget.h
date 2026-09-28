// Canvas widget: holds the document as a canonical premultiplied RGBA buffer and a
// display QImage, and runs the portable C kernels on it. The kernels assert the
// canonical-tile contract (ENG-17); this widget always feeds tight canonical tiles.
//
// File-map role: the canvas is the Qt counterpart of EditorCanvas.swift
// ("Rewrite AppKit canvas/event bridge with Qt"). This is the minimal host shell
// that proves the reused C kernels run inside Qt; full gesture/event parity is a
// later workstream.

#pragma once
#include <QImage>
#include <QWidget>
#include <cstdint>
#include <vector>

class CanvasWidget : public QWidget {
    Q_OBJECT
public:
    explicit CanvasWidget(QWidget *parent = nullptr);

    bool loadFile(const QString &path);
    bool saveFile(const QString &path, const char *format);

    bool hasImage() const { return !m_display.isNull(); }
    int imageWidth() const { return m_display.width(); }
    int imageHeight() const { return m_display.height(); }

    // In-place portable C kernels (operate on the canonical buffer).
    void applyNoise(float amount, int gaussian, int monochromatic, uint32_t seed);
    void applyGrain(double amount, double size, double roughness, uint32_t seed);
    void applyGradientMap(const uint8_t table[256 * 3]);
    void applyLensDistort(double k);

protected:
    void paintEvent(QPaintEvent *) override;

private:
    void syncDisplay(); // rebuild m_display from m_canonical

    std::vector<uint8_t> m_canonical; // premultiplied RGBA, stride == width*4
    int m_width = 0, m_height = 0;
    QImage m_display;
};