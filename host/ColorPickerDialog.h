#ifndef COLOR_PICKER_DIALOG_H
#define COLOR_PICKER_DIALOG_H

// Foreground-colour picker: saturation/value square, vertical hue strip, RGB
// fields and hex entry. Header-only and Q_OBJECT-free (SwiftPM cannot run moc).

#include <QColor>
#include <QDialog>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QImage>
#include <QLabel>
#include <QLineEdit>
#include <QMouseEvent>
#include <QPainter>
#include <QPushButton>
#include <QRegularExpression>
#include <QRegularExpressionValidator>
#include <QSpinBox>
#include <QVBoxLayout>
#include <functional>

namespace colorpicker {

class SaturationValueSquare : public QWidget {
public:
    std::function<void(double, double)> onChange;
    explicit SaturationValueSquare(QWidget *parent = nullptr) : QWidget(parent) {
        setFixedSize(236, 176);
        setCursor(Qt::CrossCursor);
    }
    void setState(double hue, double saturation, double value) {
        m_hue = hue; m_s = saturation; m_v = value;
        m_cache = QImage();
        update();
    }
protected:
    void paintEvent(QPaintEvent *) override {
        if (m_cache.size() != size()) {
            m_cache = QImage(size(), QImage::Format_RGB32);
            for (int y = 0; y < height(); ++y) {
                QRgb *row = reinterpret_cast<QRgb *>(m_cache.scanLine(y));
                for (int x = 0; x < width(); ++x) {
                    row[x] = QColor::fromHsvF(m_hue, x / double(width() - 1), 1.0 - y / double(height() - 1)).rgb();
                }
            }
        }
        QPainter p(this);
        p.setRenderHint(QPainter::Antialiasing, true);
        p.drawImage(0, 0, m_cache);
        const QPointF c(m_s * (width() - 1), (1.0 - m_v) * (height() - 1));
        p.setBrush(Qt::NoBrush);
        p.setPen(QPen(Qt::black, 2)); p.drawEllipse(c, 5.5, 5.5);
        p.setPen(QPen(Qt::white, 1.4)); p.drawEllipse(c, 5.5, 5.5);
    }
    void mousePressEvent(QMouseEvent *e) override { pick(e->position()); }
    void mouseMoveEvent(QMouseEvent *e) override { if (e->buttons() & Qt::LeftButton) pick(e->position()); }
private:
    void pick(const QPointF &pos) {
        const double s = qBound(0.0, pos.x() / (width() - 1), 1.0);
        const double v = qBound(0.0, 1.0 - pos.y() / (height() - 1), 1.0);
        if (onChange) onChange(s, v);
    }
    double m_hue = 0, m_s = 1, m_v = 1;
    QImage m_cache;
};

class HueStrip : public QWidget {
public:
    std::function<void(double)> onChange;
    explicit HueStrip(QWidget *parent = nullptr) : QWidget(parent) { setFixedSize(20, 176); setCursor(Qt::PointingHandCursor); }
    void setHue(double hue) { m_hue = hue; update(); }
protected:
    void paintEvent(QPaintEvent *) override {
        QPainter p(this);
        for (int y = 0; y < height(); ++y) {
            p.fillRect(0, y, width(), 1, QColor::fromHsvF(1.0 - y / double(height() - 1) * 0.999, 1, 1));
        }
        const int y = qRound((1.0 - m_hue) * (height() - 1));
        p.setRenderHint(QPainter::Antialiasing, true);
        p.setPen(QPen(Qt::black, 1)); p.setBrush(Qt::white);
        QPolygonF marker; marker << QPointF(0, y - 4) << QPointF(6, y) << QPointF(0, y + 4);
        p.drawPolygon(marker);
    }
    void mousePressEvent(QMouseEvent *e) override { pick(e->position().y()); }
    void mouseMoveEvent(QMouseEvent *e) override { if (e->buttons() & Qt::LeftButton) pick(e->position().y()); }
private:
    void pick(double y) { if (onChange) onChange(qBound(0.0, 1.0 - y / (height() - 1), 0.999)); }
    double m_hue = 0;
};

}  // namespace colorpicker

class ColorPickerDialog : public QDialog {
public:
    ColorPickerDialog(const QColor &initial, const QString &title, QWidget *parent = nullptr) : QDialog(parent) {
        setObjectName("colorPickerDialog");
        setWindowTitle(title);
        m_color = initial.isValid() ? initial : QColor(Qt::black);
        m_h = m_color.hsvHueF(); m_s = m_color.hsvSaturationF(); m_v = m_color.valueF();
        if (m_h < 0) m_h = 0;

        auto *root = new QHBoxLayout(this);
        root->setSpacing(12);
        m_square = new colorpicker::SaturationValueSquare(this);
        m_strip = new colorpicker::HueStrip(this);
        m_square->onChange = [this](double s, double v) { m_s = s; m_v = v; commit(); };
        m_strip->onChange = [this](double h) { m_h = h; commit(); };
        root->addWidget(m_square);
        root->addWidget(m_strip);

        auto *side = new QVBoxLayout;
        side->setSpacing(6);
        m_swatch = new QLabel(this);
        m_swatch->setObjectName("colorPicker.swatch");
        m_swatch->setFixedHeight(34);
        side->addWidget(m_swatch);
        auto *grid = new QGridLayout;
        grid->setHorizontalSpacing(8);
        const char *names[3] = {"R", "G", "B"};
        for (int i = 0; i < 3; ++i) {
            m_channel[i] = new QSpinBox(this);
            m_channel[i]->setObjectName(QString("colorPicker.%1").arg(names[i]));
            m_channel[i]->setRange(0, 255);
            m_channel[i]->setFixedWidth(58);
            grid->addWidget(new QLabel(names[i], this), i, 0);
            grid->addWidget(m_channel[i], i, 1);
            connect(m_channel[i], &QSpinBox::valueChanged, this, [this] {
                if (m_updating) return;
                QColor c(m_channel[0]->value(), m_channel[1]->value(), m_channel[2]->value());
                const double h = c.hsvHueF();
                if (h >= 0) m_h = h;
                m_s = c.hsvSaturationF(); m_v = c.valueF(); commit();
            });
        }
        m_hex = new QLineEdit(this);
        m_hex->setObjectName("colorPicker.hex");
        m_hex->setMaxLength(7);
        m_hex->setFixedWidth(84);
        m_hex->setValidator(new QRegularExpressionValidator(QRegularExpression("#?[0-9A-Fa-f]{0,6}"), m_hex));
        grid->addWidget(new QLabel("#", this), 3, 0);
        grid->addWidget(m_hex, 3, 1);
        connect(m_hex, &QLineEdit::editingFinished, this, [this] {
            QString text = m_hex->text();
            if (!text.startsWith('#')) text.prepend('#');
            const QColor c(text);
            if (!c.isValid()) { commit(); return; }
            const double h = c.hsvHueF();
            if (h >= 0) m_h = h;
            m_s = c.hsvSaturationF(); m_v = c.valueF(); commit();
        });
        side->addLayout(grid);
        side->addStretch();
        auto *hint = new QLabel(tr("Drag the square to choose"), this);
        hint->setStyleSheet("color: #7a7a80; font-size: 10px;");
        side->addWidget(hint);
        auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
        side->addWidget(buttons);
        connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
        connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
        root->addLayout(side);
        commit();
    }

    QColor color() const { return m_color; }

    // Returns an invalid colour when cancelled.
    static QColor getColor(const QColor &initial, QWidget *parent, const QString &title) {
        ColorPickerDialog dialog(initial, title, parent);
        return dialog.exec() == QDialog::Accepted ? dialog.color() : QColor();
    }

private:
    void commit() {
        m_color = QColor::fromHsvF(m_h, m_s, m_v);
        m_updating = true;
        m_square->setState(m_h, m_s, m_v);
        m_strip->setHue(m_h);
        m_channel[0]->setValue(m_color.red());
        m_channel[1]->setValue(m_color.green());
        m_channel[2]->setValue(m_color.blue());
        m_hex->setText(m_color.name(QColor::HexRgb).toUpper().mid(1));
        m_swatch->setStyleSheet(QString("background: %1; border: 1px solid #55565a; border-radius: 4px;").arg(m_color.name()));
        m_updating = false;
    }

    QColor m_color;
    double m_h = 0, m_s = 0, m_v = 0;
    bool m_updating = false;
    colorpicker::SaturationValueSquare *m_square = nullptr;
    colorpicker::HueStrip *m_strip = nullptr;
    QLabel *m_swatch = nullptr;
    QSpinBox *m_channel[3] = {nullptr, nullptr, nullptr};
    QLineEdit *m_hex = nullptr;
};

#endif
