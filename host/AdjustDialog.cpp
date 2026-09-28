#include "EditorDialogs.h"
#include "QtPlatformServices.h"

#include <QButtonGroup>
#include <QCheckBox>
#include <QColor>
#include <QColorDialog>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QLabel>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QSlider>
#include <QSignalBlocker>
#include <cmath>
#include <algorithm>
#include <functional>
#include <QPushButton>
#include <QTableWidget>
#include <QTimer>
#include <QToolButton>
#include <QVBoxLayout>
#include <memory>
#include <vector>

// The Swift LayerAdjustment model decodes hue/saturation/lightness/colorize,
// levels, and curves unconditionally (synthesized Codable, non-optional fields);
// the per-kind settings objects that follow are optional and only sent when set.
static QJsonObject levelRange(double black, double gamma, double white,
                              double outputBlack, double outputWhite) {
    return {{"black", black}, {"gamma", gamma}, {"white", white},
            {"outputBlack", outputBlack}, {"outputWhite", outputWhite}};
}

static QJsonArray levelRangesIdentity() {
    QJsonArray ranges;
    for (int i = 0; i < 4; ++i) ranges.append(levelRange(0, 1, 255, 0, 255));
    return ranges;
}

static QJsonArray curvePointsIdentity() {
    QJsonArray points;
    points.append(QJsonObject{{"x", 0}, {"y", 0}});
    points.append(QJsonObject{{"x", 255}, {"y", 255}});
    return points;
}

static QJsonArray curveChannelsIdentity() {
    QJsonArray channels;
    for (int i = 0; i < 4; ++i) channels.append(curvePointsIdentity());
    return channels;
}

static QString hueBandKey(int index) {
    static const QStringList keys = {"Master", "Reds", "Yellows", "Greens", "Cyans", "Blues", "Magentas"};
    return keys[index];
}

static QJsonObject hueBandDefault(int index) {
    // Photoshop's starting hue bands for the seven ColorRange presets.
    static const QList<QList<double>> bands = {
        {0, 0, 360, 360}, {315, 345, 15, 45}, {15, 45, 75, 105}, {75, 105, 135, 165},
        {135, 165, 195, 225}, {195, 225, 255, 285}, {255, 285, 315, 345}};
    const QList<double> &b = bands[index];
    return {{"falloffStart", b[0]}, {"rangeStart", b[1]}, {"rangeEnd", b[2]}, {"falloffEnd", b[3]}};
}


// ---- Shared dialog widgets ----------------------------------------------------

// A numeric field paired with a slider, both ways in sync.
static QWidget *sliderRow(QDoubleSpinBox *box, QWidget *parent) {
    auto *row = new QWidget(parent);
    auto *h = new QHBoxLayout(row);
    h->setContentsMargins(0, 0, 0, 0);
    h->setSpacing(10);
    auto *slider = new QSlider(Qt::Horizontal, row);
    const double scale = std::pow(10.0, box->decimals());
    slider->setRange(qRound(box->minimum() * scale), qRound(box->maximum() * scale));
    slider->setValue(qRound(box->value() * scale));
    QObject::connect(slider, &QSlider::valueChanged, box, [=](int v) {
        if (!qFuzzyCompare(box->value() + 1, v / scale + 1)) box->setValue(v / scale);
    });
    QObject::connect(box, qOverload<double>(&QDoubleSpinBox::valueChanged), slider, [=](double v) {
        const QSignalBlocker blocker(slider);
        slider->setValue(qRound(v * scale));
    });
    box->setParent(row);
    box->setFixedWidth(64);
    box->setButtonSymbols(QAbstractSpinBox::NoButtons);
    box->setAlignment(Qt::AlignRight);
    h->addWidget(slider, 1);
    h->addWidget(box);
    row->setFocusProxy(box);
    return row;
}

// Hue/Saturation range strips: the reference rainbow above, the shifted result below,
// with the selected colour range's fall-off / range markers.
class HueRangeBars : public QWidget {
public:
    explicit HueRangeBars(QWidget *parent = nullptr) : QWidget(parent) { setFixedHeight(52); }
    void setState(double shift, int band) { m_shift = shift; m_band = band; update(); }
protected:
    void paintEvent(QPaintEvent *) override {
        QPainter p(this);
        const QRect top(0, 4, width(), 12), bottom(0, 28, width(), 12);
        for (int x = 0; x < width(); ++x) {
            const double t = x / double(std::max(1, width() - 1));
            double shifted = std::fmod(t + m_shift / 360.0 + 1.0, 1.0);
            p.fillRect(x, top.top(), 1, top.height(), QColor::fromHsvF(std::min(t, 0.999), 0.9, 0.95));
            p.fillRect(x, bottom.top(), 1, bottom.height(), QColor::fromHsvF(std::min(shifted, 0.999), 0.9, 0.95));
        }
        p.setPen(QColor(0x18, 0x18, 0x1a)); p.setBrush(Qt::NoBrush);
        p.drawRect(top.adjusted(0, 0, -1, 0)); p.drawRect(bottom.adjusted(0, 0, -1, 0));
        if (m_band > 0) {
            const QJsonObject b = hueBandDefault(m_band);
            p.setPen(QColor(0xf2, 0xf2, 0xf5)); p.setBrush(QColor(0xf2, 0xf2, 0xf5));
            const char *keys[4] = {"falloffStart", "rangeStart", "rangeEnd", "falloffEnd"};
            for (int i = 0; i < 4; ++i) {
                double deg = std::fmod(b.value(keys[i]).toDouble() + 360.0, 360.0);
                const double x = deg / 360.0 * (width() - 1);
                if (i == 0 || i == 3) { p.drawLine(QPointF(x, bottom.bottom() + 2), QPointF(x, bottom.bottom() + 8)); }
                else { QPolygonF tri; tri << QPointF(x - 3, bottom.bottom() + 9) << QPointF(x + 3, bottom.bottom() + 9) << QPointF(x, bottom.bottom() + 3); p.drawPolygon(tri); }
            }
        }
    }
private:
    double m_shift = 0;
    int m_band = 0;
};

// Levels histogram with draggable black / gamma / white input markers.
class LevelsHistogram : public QWidget {
public:
    std::function<void(double black, double gamma, double white)> onChange;
    explicit LevelsHistogram(QWidget *parent = nullptr) : QWidget(parent) {
        setFixedHeight(112);
        setMinimumWidth(280);
    }
    void setBins(const std::vector<double> &bins) { m_bins = bins; update(); }
    void setMarkers(double black, double gamma, double white) { m_b = black; m_g = gamma; m_w = white; update(); }
protected:
    QRectF plot() const { return QRectF(6, 4, width() - 12, height() - 22); }
    double xFor(double level) const { return plot().left() + level / 255.0 * plot().width(); }
    double levelFor(double x) const { return qBound(0.0, (x - plot().left()) / plot().width() * 255.0, 255.0); }
    double gammaX() const { return xFor(m_b + (m_w - m_b) * std::pow(0.5, m_g)); }
    void paintEvent(QPaintEvent *) override {
        QPainter p(this);
        p.setRenderHint(QPainter::Antialiasing, true);
        const QRectF r = plot();
        p.fillRect(r, QColor(0x14, 0x14, 0x16));
        double peak = 0;
        for (double v : m_bins) peak = std::max(peak, v);
        if (peak > 0 && m_bins.size() == 256) {
            QPainterPath path;
            path.moveTo(r.left(), r.bottom());
            for (int i = 0; i < 256; ++i) {
                path.lineTo(r.left() + i / 255.0 * r.width(), r.bottom() - std::sqrt(m_bins[i] / peak) * (r.height() - 2));
            }
            path.lineTo(r.right(), r.bottom());
            p.setPen(Qt::NoPen); p.setBrush(QColor(0x9a, 0x9a, 0xa0));
            p.drawPath(path);
        }
        auto marker = [&](double x, const QColor &fill) {
            QPolygonF tri; tri << QPointF(x, r.bottom() + 2) << QPointF(x - 5, r.bottom() + 12) << QPointF(x + 5, r.bottom() + 12);
            p.setPen(QPen(QColor(0x60, 0x60, 0x66), 1)); p.setBrush(fill); p.drawPolygon(tri);
        };
        marker(xFor(m_b), QColor(0x10, 0x10, 0x10));
        marker(gammaX(), QColor(0x80, 0x80, 0x80));
        marker(xFor(m_w), QColor(0xf2, 0xf2, 0xf5));
    }
    void mousePressEvent(QMouseEvent *e) override {
        const double x = e->position().x();
        const double db = std::abs(x - xFor(m_b)), dg = std::abs(x - gammaX()), dw = std::abs(x - xFor(m_w));
        m_drag = (dg <= db && dg <= dw) ? 1 : (db <= dw ? 0 : 2);
        moveDrag(x);
    }
    void mouseMoveEvent(QMouseEvent *e) override { if (m_drag >= 0) moveDrag(e->position().x()); }
    void mouseReleaseEvent(QMouseEvent *) override { m_drag = -1; }
private:
    void moveDrag(double x) {
        double b = m_b, g = m_g, w = m_w;
        const double level = levelFor(x);
        if (m_drag == 0) b = std::min(level, w - 1);
        else if (m_drag == 2) w = std::max(level, b + 1);
        else {
            const double t = qBound(0.02, (level - b) / std::max(1.0, w - b), 0.98);
            g = qBound(0.1, std::log(t) / std::log(0.5), 9.99);
        }
        if (onChange) onChange(std::round(b), std::round(g * 100) / 100.0, std::round(w));
    }
    std::vector<double> m_bins;
    double m_b = 0, m_g = 1, m_w = 255;
    int m_drag = -1;
};


// Curves graph: drag points, click to add, double-click a point to remove it.
class CurveEditor : public QWidget {
public:
    std::function<void(const std::vector<QPointF> &)> onChange;
    explicit CurveEditor(QWidget *parent = nullptr) : QWidget(parent) { setFixedSize(260, 260); setCursor(Qt::CrossCursor); }
    void setPoints(const std::vector<QPointF> &pts) { m_pts = pts; update(); }
protected:
    QRectF area() const { return QRectF(6, 6, width() - 12, height() - 12); }
    QPointF toWidget(const QPointF &pt) const { return QPointF(area().left() + pt.x() / 255.0 * area().width(), area().bottom() - pt.y() / 255.0 * area().height()); }
    QPointF toValue(const QPointF &w) const {
        return QPointF(qBound(0.0, (w.x() - area().left()) / area().width() * 255.0, 255.0),
                       qBound(0.0, (area().bottom() - w.y()) / area().height() * 255.0, 255.0));
    }
    int hit(const QPointF &w) const {
        for (int i = 0; i < int(m_pts.size()); ++i) if (QLineF(w, toWidget(m_pts[i])).length() <= 8) return i;
        return -1;
    }
    void paintEvent(QPaintEvent *) override {
        QPainter p(this);
        p.setRenderHint(QPainter::Antialiasing, true);
        const QRectF r = area();
        p.fillRect(r, QColor(0x14, 0x14, 0x16));
        p.setPen(QPen(QColor(0x2e, 0x2e, 0x32), 1));
        for (int i = 1; i < 4; ++i) {
            p.drawLine(QPointF(r.left() + r.width() * i / 4, r.top()), QPointF(r.left() + r.width() * i / 4, r.bottom()));
            p.drawLine(QPointF(r.left(), r.top() + r.height() * i / 4), QPointF(r.right(), r.top() + r.height() * i / 4));
        }
        p.setPen(QPen(QColor(0x44, 0x44, 0x4a), 1, Qt::DashLine));
        p.drawLine(r.bottomLeft(), r.topRight());
        std::vector<QPointF> sorted = m_pts;
        std::sort(sorted.begin(), sorted.end(), [](const QPointF &a, const QPointF &b) { return a.x() < b.x(); });
        if (sorted.size() >= 2) {
            // Catmull-Rom style smoothing through the points for display.
            QPainterPath path;
            path.moveTo(toWidget(sorted.front()));
            for (size_t i = 0; i + 1 < sorted.size(); ++i) {
                const QPointF p0 = toWidget(sorted[i == 0 ? 0 : i - 1]), p1 = toWidget(sorted[i]), p2 = toWidget(sorted[i + 1]),
                              p3 = toWidget(sorted[i + 2 < sorted.size() ? i + 2 : i + 1]);
                path.cubicTo(p1 + (p2 - p0) / 6.0, p2 - (p3 - p1) / 6.0, p2);
            }
            p.setPen(QPen(QColor(0xf2, 0xf2, 0xf5), 1.6)); p.setBrush(Qt::NoBrush);
            p.drawPath(path);
        }
        for (size_t i = 0; i < m_pts.size(); ++i) {
            p.setPen(QPen(QColor(0x10, 0x10, 0x12), 1.4));
            p.setBrush(int(i) == m_drag ? QColor(0x0a, 0x84, 0xff) : QColor(0xf2, 0xf2, 0xf5));
            p.drawEllipse(toWidget(m_pts[i]), 4.5, 4.5);
        }
    }
    void mousePressEvent(QMouseEvent *e) override {
        m_drag = hit(e->position());
        if (m_drag < 0) {
            m_pts.push_back(toValue(e->position()));
            m_drag = int(m_pts.size()) - 1;
        }
        emitChange();
    }
    void mouseMoveEvent(QMouseEvent *e) override {
        if (m_drag < 0 || !(e->buttons() & Qt::LeftButton)) return;
        QPointF v = toValue(e->position());
        v.setX(std::round(v.x() * 10) / 10.0); v.setY(std::round(v.y() * 10) / 10.0);
        m_pts[m_drag] = v;
        emitChange();
    }
    void mouseReleaseEvent(QMouseEvent *) override { m_drag = -1; update(); }
    void mouseDoubleClickEvent(QMouseEvent *e) override {
        const int i = hit(e->position());
        if (i >= 0 && m_pts.size() > 2) { m_pts.erase(m_pts.begin() + i); m_drag = -1; emitChange(); }
    }
private:
    void emitChange() { update(); if (onChange) onChange(m_pts); }
    std::vector<QPointF> m_pts;
    int m_drag = -1;
};

// Plain black-to-white ramp under the output fields.
class OutputRamp : public QWidget {
public:
    explicit OutputRamp(QWidget *parent = nullptr) : QWidget(parent) { setFixedHeight(14); }
protected:
    void paintEvent(QPaintEvent *) override {
        QPainter p(this);
        QLinearGradient g(0, 0, width(), 0);
        g.setColorAt(0, Qt::black); g.setColorAt(1, Qt::white);
        p.fillRect(rect().adjusted(6, 0, -6, 0), g);
    }
};

AdjustDialog::AdjustDialog(const QString &kind, Submit submit, QWidget *parent, AdjustServices services) : QDialog(parent) {
    const HistogramProvider histogram = services.histogram;
    const AdjustServices::SampleRequest requestSample = services.requestSample;
    std::shared_ptr<IColorPickerService> colors = services.colors ? services.colors : PlatformServices::qtDefaults().colors;
    setObjectName("adjustDialog"); setWindowTitle(kind);
    setMinimumWidth(kind == "Levels" ? 460 : 480);
    // Photoshop-style two-column shell: controls on the left, OK/Cancel stacked
    // in a right-hand rail (matches docs/references/{levels,hue-saturation}.png).
    auto *root = new QHBoxLayout(this);
    auto *layout = new QVBoxLayout;
    layout->setSpacing(10);
    root->addLayout(layout, 1);
    auto *form = new QFormLayout;
    form->setLabelAlignment(Qt::AlignLeft);
    form->setHorizontalSpacing(12);
    layout->addLayout(form);
    // Kind-specific controls go here, above Preview and the button row.
    auto *extras = new QVBoxLayout;
    extras->setSpacing(8);
    layout->addLayout(extras);

    auto adjustment = std::make_shared<QJsonObject>();
    adjustment->insert("kind", kind);

    // Required non-optional LayerAdjustment fields (identity by default).
    adjustment->insert("hue", 0); adjustment->insert("saturation", 0);
    adjustment->insert("lightness", 0); adjustment->insert("colorize", false);
    adjustment->insert("levels", QJsonObject{{"channel", "RGB"}, {"ranges", levelRangesIdentity()}});
    // LayerAdjustment's synthesized decoder requires `curves` even when the sheet
    // is not Curves; without it every preview fails with "Invalid command JSON".
    adjustment->insert("curves", QJsonObject{{"channel", "RGB"}, {"channels", curveChannelsIdentity()}});

    auto *debounce = new QTimer(this);
    debounce->setSingleShot(true); debounce->setInterval(120);
    auto changed = [=] {
        QJsonObject command{{"action", "adjustmentPreview"}, {"adjustment", *adjustment}};
        const bool ok = submit(command);
        if (ok) setProperty("previewFailed", false);
        return ok;
    };
    auto *error = new QLabel(this); error->setWordWrap(true); layout->addWidget(error);

    // Right-hand rail: OK/Cancel stacked, Preview pinned to the bottom (as in the macOS reference).
    auto *rail = new QVBoxLayout;
    rail->setSpacing(8);
    root->addLayout(rail);
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    buttons->setOrientation(Qt::Vertical);
    rail->addWidget(buttons);
    rail->addStretch(1);
    auto *preview = new QCheckBox(tr("Preview"), this);
    preview->setObjectName("preview"); preview->setChecked(true);
    rail->addWidget(preview);

    if (kind == "Levels") {
        auto *channel = new QComboBox(this);
        channel->setObjectName("channel"); channel->setAccessibleName("Channel");
        channel->addItems({"RGB", "Red", "Green", "Blue"});
        form->addRow(tr("&Channel"), channel);
        auto *hist = new LevelsHistogram(this);
        extras->addWidget(hist);
        const QStringList names = {"Black", "Gamma", "White", "Output Black", "Output White"};
        const QStringList captions = {tr("Input black"), tr("Gamma"), tr("Input white"), tr("Output black"), tr("Output white")};
        const double defaults[5] = {0, 1, 255, 0, 255};
        auto ranges = std::make_shared<QJsonArray>(levelRangesIdentity());
        auto *fields = new QDoubleSpinBox *[5];
        for (int i = 0; i < 5; ++i) {
            fields[i] = new QDoubleSpinBox(this);
            QString name = names[i];
            fields[i]->setObjectName(name.replace(' ', "")); fields[i]->setAccessibleName(names[i]);
            fields[i]->setRange(i == 1 ? 0.01 : -255, i == 1 ? 9.99 : 255);
            fields[i]->setDecimals(i == 1 ? 2 : 0); fields[i]->setValue(defaults[i]);
            fields[i]->setKeyboardTracking(false);
            fields[i]->setButtonSymbols(QAbstractSpinBox::NoButtons);
            fields[i]->setAlignment(Qt::AlignRight);
        }
        auto fieldGrid = [&](std::initializer_list<int> which) {
            auto *grid = new QGridLayout;
            grid->setHorizontalSpacing(12);
            int col = 0;
            for (int i : which) {
                auto *cap = new QLabel(captions[i], this);
                cap->setStyleSheet("color: #a0a0a5; font-size: 11px;");
                cap->setBuddy(fields[i]);
                grid->addWidget(cap, 0, col);
                grid->addWidget(fields[i], 1, col);
                grid->setColumnStretch(col, 1);
                ++col;
            }
            return grid;
        };
        extras->addLayout(fieldGrid({0, 1, 2}));
        extras->addWidget(new OutputRamp(this));
        extras->addLayout(fieldGrid({3, 4}));
        auto syncHistogram = [=] {
            hist->setMarkers(fields[0]->value(), fields[1]->value(), fields[2]->value());
        };
        auto loadBins = [=](int index) { if (histogram) hist->setBins(histogram(index)); };
        for (int i = 0; i < 5; ++i) {
            connect(fields[i], qOverload<double>(&QDoubleSpinBox::valueChanged), this,
                    [=](double v) {
                        static const char *const keys[5] = {"black", "gamma", "white", "outputBlack", "outputWhite"};
                        QJsonObject range = ranges->at(channel->currentIndex()).toObject();
                        range.insert(keys[i], v);
                        ranges->replace(channel->currentIndex(), range);
                        adjustment->insert("levels", QJsonObject{{"channel", channel->currentText()}, {"ranges", *ranges}});
                        syncHistogram();
                        debounce->start();
                    });
        }
        hist->onChange = [=](double b, double g, double w) {
            fields[0]->setValue(b); fields[1]->setValue(g); fields[2]->setValue(w);
        };
        connect(channel, qOverload<int>(&QComboBox::currentIndexChanged), this, [=](int index) {
            const QJsonObject range = ranges->at(index).toObject();
            fields[0]->setValue(range.value("black").toDouble());
            fields[1]->setValue(range.value("gamma").toDouble());
            fields[2]->setValue(range.value("white").toDouble());
            fields[3]->setValue(range.value("outputBlack").toDouble());
            fields[4]->setValue(range.value("outputWhite").toDouble());
            loadBins(index);
            syncHistogram();
        });
        auto *reset = new QPushButton(tr("Reset"), this);
        reset->setObjectName("reset");
        extras->addWidget(reset, 0, Qt::AlignRight);
        connect(reset, &QPushButton::clicked, this, [=] {
            for (int i = 0; i < 5; ++i) fields[i]->setValue(defaults[i]);
        });
        loadBins(0);
        syncHistogram();

        // Writes `ranges` into the adjustment and refreshes the visible fields for the current channel.
        auto commitRanges = [=] {
            adjustment->insert("levels", QJsonObject{{"channel", channel->currentText()}, {"ranges", *ranges}});
            const QJsonObject range = ranges->at(channel->currentIndex()).toObject();
            fields[0]->setValue(range.value("black").toDouble());
            fields[1]->setValue(range.value("gamma").toDouble());
            fields[2]->setValue(range.value("white").toDouble());
            fields[3]->setValue(range.value("outputBlack").toDouble());
            fields[4]->setValue(range.value("outputWhite").toDouble());
            syncHistogram();
            debounce->start();
        };

        // Auto: black/white points from the 0.1% tails; "Color" per channel, "Contrast" one shared interval,
        // "neutral" also sets each channel's gamma so its mean lands on mid-grey (mirrors LevelsAuto).
        auto endpoints = [](const std::vector<double> &bins, double &low, double &high) {
            double total = 0; for (double v : bins) total += v;
            if (total <= 0 || bins.size() != 256) return false;
            double sum = 0; int lo = 0, hi = 255;
            for (int i = 0; i < 256; ++i) { sum += bins[i]; if (sum > total * 0.001) { lo = i; break; } }
            sum = 0;
            for (int i = 255; i >= 0; --i) { sum += bins[i]; if (sum > total * 0.001) { hi = i; break; } }
            low = lo; high = hi;
            return lo < hi;
        };
        auto autoLevels = [=](int mode) {
            if (!histogram) return;
            QJsonArray next = levelRangesIdentity();
            if (mode == 0) {
                double lowest = 255, highest = 0; bool any = false;
                for (int c = 1; c <= 3; ++c) {
                    double lo, hi;
                    if (endpoints(histogram(c), lo, hi)) { lowest = std::min(lowest, lo); highest = std::max(highest, hi); any = true; }
                }
                if (any && lowest < highest) next.replace(0, levelRange(lowest, 1, highest, 0, 255));
            } else {
                for (int c = 1; c <= 3; ++c) {
                    const std::vector<double> bins = histogram(c);
                    double lo, hi;
                    if (!endpoints(bins, lo, hi)) continue;
                    double gamma = 1;
                    if (mode == 2) {
                        double total = 0, weighted = 0;
                        for (int i = 0; i < 256; ++i) { total += bins[i]; weighted += bins[i] * std::clamp((i - lo) / (hi - lo), 0.0, 1.0); }
                        const double mean = weighted / total;
                        if (mean > 0 && mean < 1) gamma = std::clamp(std::log(mean) / std::log(0.5), 0.1, 9.99);
                    }
                    next.replace(c, levelRange(lo, gamma, hi, 0, 255));
                }
            }
            *ranges = next;
            commitRanges();
        };

        // Eyedroppers: calibrate all three channels from one clicked pixel (mirrors LevelsSettings.sampling).
        auto sampleLevels = [=](int mode, const QColor &color) {
            QJsonArray next = *ranges;
            next.replace(0, levelRange(0, 1, 255, 0, 255));
            const double components[3] = {color.redF() * 255, color.greenF() * 255, color.blueF() * 255};
            for (int c = 1; c <= 3; ++c) {
                QJsonObject range = next.at(c).toObject();
                double black = range.value("black").toDouble(), white = range.value("white").toDouble(), gamma = range.value("gamma").toDouble(1);
                const double v = components[c - 1];
                if (mode == 0) black = std::min(white - 1, std::max(0.0, v));
                else if (mode == 2) white = std::max(black + 1, std::min(255.0, v));
                else {
                    const double fraction = (v - black) / (white - black);
                    if (fraction > 0 && fraction < 1) gamma = std::clamp(std::log(fraction) / std::log(0.5), 0.1, 9.99);
                }
                next.replace(c, levelRange(black, gamma, white, 0, 255));
            }
            *ranges = next;
            commitRanges();
        };

        auto *autoRow = new QHBoxLayout;
        auto *autoLabel = new QLabel(tr("Auto"), this);
        autoLabel->setStyleSheet("color: #a0a0a5; font-size: 11px;");
        autoRow->addWidget(autoLabel);
        const QStringList autoNames = {tr("Contrast"), tr("Color"), tr("Color + neutral midtones")};
        for (int i = 0; i < 3; ++i) {
            auto *button = new QPushButton(autoNames[i], this);
            button->setObjectName(QString("auto.%1").arg(i));
            button->setEnabled(bool(histogram));
            connect(button, &QPushButton::clicked, this, [=] { autoLevels(i); });
            autoRow->addWidget(button);
        }
        autoRow->addStretch();
        extras->addLayout(autoRow);

        auto *sampleRow = new QHBoxLayout;
        auto *sampleLabel = new QLabel(tr("Sample"), this);
        sampleLabel->setStyleSheet("color: #a0a0a5; font-size: 11px;");
        sampleRow->addWidget(sampleLabel);
        const QStringList sampleNames = {tr("Black"), tr("Gray"), tr("White")};
        for (int i = 0; i < 3; ++i) {
            auto *button = new QPushButton(sampleNames[i], this);
            button->setObjectName(QString("sample.%1").arg(i));
            button->setToolTip(tr("Click, then click a pixel on the canvas to set the %1 point").arg(sampleNames[i].toLower()));
            button->setEnabled(bool(requestSample));
            connect(button, &QPushButton::clicked, this, [=] {
                // Step aside so the canvas receives the click, then come back.
                setWindowModality(Qt::NonModal); show();
                requestSample([=](const QColor &color) {
                    setWindowModality(Qt::ApplicationModal); show(); raise();
                    if (color.isValid()) sampleLevels(i, color);
                });
            });
            sampleRow->addWidget(button);
        }
        sampleRow->addStretch();
        extras->addLayout(sampleRow);
    }
    if (kind == "Hue/Saturation") {
        auto *range = new QComboBox(this);
        range->setObjectName("range"); range->setAccessibleName("Range");
        for (int i = 0; i < 7; ++i) range->addItem(hueBandKey(i));
        form->addRow(tr("&Range"), range);
        // Quick-pick swatches (docs/references/hue-saturation.png): Master (hand/wheel) plus the six colour
        // bands, each a coloured button that jumps `range` to that band — the combo above stays as the
        // accessible/keyboard-driven equivalent, both drive the same index.
        auto *swatchRow = new QHBoxLayout;
        swatchRow->setSpacing(4);
        auto *swatchGroup = new QButtonGroup(this);
        swatchGroup->setExclusive(true);
        static const QColor swatchColors[7] = {
            QColor(0x3a, 0x3a, 0x3e), QColor(0xe0, 0x3c, 0x31), QColor(0xf2, 0xc9, 0x1c),
            QColor(0x4c, 0xaf, 0x50), QColor(0x26, 0xc6, 0xda), QColor(0x3f, 0x51, 0xb5), QColor(0xe0, 0x3f, 0xc4)};
        for (int i = 0; i < 7; ++i) {
            auto *swatch = new QToolButton(this);
            swatch->setCheckable(true);
            swatch->setChecked(i == 0);
            swatch->setFixedSize(28, 22);
            swatch->setToolTip(hueBandKey(i));
            swatch->setObjectName(QString("swatch.%1").arg(i));
            const QColor &c = swatchColors[i];
            swatch->setStyleSheet(QString("QToolButton { background: %1; border: 1px solid #18181a; border-radius: 3px; }"
                                          "QToolButton:checked { border: 2px solid #f2f2f5; }").arg(c.name()));
            swatchGroup->addButton(swatch, i);
            swatchRow->addWidget(swatch);
        }
        swatchRow->addStretch();
        extras->addLayout(swatchRow);
        connect(swatchGroup, &QButtonGroup::idClicked, this, [=](int id) { range->setCurrentIndex(id); });
        connect(range, qOverload<int>(&QComboBox::currentIndexChanged), this, [=](int index) {
            if (auto *button = swatchGroup->button(index)) button->setChecked(true);
        });
        auto *hue = new QDoubleSpinBox(this), *sat = new QDoubleSpinBox(this), *light = new QDoubleSpinBox(this);
        auto addHsv = [=](QDoubleSpinBox *box, const QString &label, double min, double max) {
            box->setObjectName(label); box->setAccessibleName(label);
            box->setRange(min, max); box->setDecimals(0); box->setKeyboardTracking(false);
            form->addRow(tr("&%1").arg(label), sliderRow(box, this));
        };
        addHsv(hue, "Hue", -180, 180); addHsv(sat, "Saturation", -100, 100); addHsv(light, "Lightness", -100, 100);
        auto *bars = new HueRangeBars(this);
        extras->addWidget(bars);
        auto *colorize = new QCheckBox(tr("Colorize"), this), *invert = new QCheckBox(tr("Apply outside this range instead"), this);
        auto *hsvReset = new QPushButton(tr("Reset"), this);
        hsvReset->setObjectName("reset");
        auto *hsvOptions = new QHBoxLayout;
        hsvOptions->addWidget(colorize); hsvOptions->addWidget(invert); hsvOptions->addStretch(); hsvOptions->addWidget(hsvReset);
        extras->addLayout(hsvOptions);
        connect(hsvReset, &QPushButton::clicked, this, [=] {
            hue->setValue(0); sat->setValue(0); light->setValue(0); colorize->setChecked(false); invert->setChecked(false);
        });
        auto refreshBars = [=] { bars->setState(hue->value(), range->currentIndex()); };
        connect(hue, qOverload<double>(&QDoubleSpinBox::valueChanged), this, [=](double) { refreshBars(); });
        connect(range, qOverload<int>(&QComboBox::currentIndexChanged), this, [=](int) { refreshBars(); });
        auto build = [=] {
            // Swift dicts keyed by enums encode as arrays of alternating key/value.
            QJsonArray adjustments;
            QJsonObject master{{"hue", hue->value()}, {"saturation", sat->value()}, {"lightness", light->value()}};
            adjustments.append("Master"); adjustments.append(master);
            if (range->currentIndex() != 0) {
                QJsonObject band = master;
                band.insert("hue", 0.0);
                adjustments.append(hueBandKey(range->currentIndex())); adjustments.append(band);
            }
            QJsonArray bands;
            for (int i = 0; i < 7; ++i) { bands.append(hueBandKey(i)); bands.append(hueBandDefault(i)); }
            adjustment->insert("hsvSettings", QJsonObject{
                {"range", range->currentText()}, {"colorize", colorize->isChecked()},
                {"invertRange", invert->isChecked()}, {"adjustments", adjustments}, {"bands", bands}});
        };
        auto connectAll = [=](auto *box) { connect(box, qOverload<double>(&QDoubleSpinBox::valueChanged), this, [=](double) { build(); debounce->start(); }); };
        connectAll(hue); connectAll(sat); connectAll(light);
        connect(colorize, &QCheckBox::toggled, this, [=](bool) { build(); debounce->start(); });
        connect(invert, &QCheckBox::toggled, this, [=](bool) { build(); debounce->start(); });
        connect(range, qOverload<int>(&QComboBox::currentIndexChanged), this, [=](int) { build(); debounce->start(); });
        build();
    }
    if (kind == "Curves") {
        auto *channel = new QComboBox(this);
        channel->setObjectName("channel"); channel->setAccessibleName("Channel");
        channel->addItems({"RGB", "Red", "Green", "Blue"});
        form->addRow(tr("&Channel"), channel);
        auto *graph = new CurveEditor(this);
        graph->setObjectName("curveGraph");
        extras->addWidget(graph, 0, Qt::AlignHCenter);
        auto *table = new QTableWidget(2, 2, this);
        table->setObjectName("points"); table->setAccessibleName("Points");
        table->setHorizontalHeaderLabels({tr("Input"), tr("Output")});
        table->setAcceptDrops(false);
        table->setMaximumHeight(110);
        extras->addWidget(table);
        auto *rowButtons = new QHBoxLayout;
        auto *addBtn = new QPushButton(tr("Add"), this), *delBtn = new QPushButton(tr("Delete"), this),
             *resetBtn = new QPushButton(tr("Reset channel"), this);
        rowButtons->addWidget(addBtn); rowButtons->addWidget(delBtn); rowButtons->addStretch(); rowButtons->addWidget(resetBtn);
        extras->addLayout(rowButtons);
        auto channels = std::make_shared<QJsonArray>(curveChannelsIdentity());
        auto syncing = std::make_shared<bool>(false);
        auto pointsFromTable = [=] {
            std::vector<QPointF> pts;
            for (int i = 0; i < table->rowCount(); ++i) {
                auto *x = qobject_cast<QDoubleSpinBox *>(table->cellWidget(i, 0));
                auto *y = qobject_cast<QDoubleSpinBox *>(table->cellWidget(i, 1));
                if (x && y) pts.push_back(QPointF(x->value(), y->value()));
            }
            return pts;
        };
        // sync: table -> model + graph + debounced preview.
        std::shared_ptr<std::function<void()>> sync = std::make_shared<std::function<void()>>();
        auto addRow = [=](int row, double xv, double yv) {
            auto *x = new QDoubleSpinBox(table), *y = new QDoubleSpinBox(table);
            x->setRange(0, 255); y->setRange(0, 255); x->setDecimals(1); y->setDecimals(1);
            x->setKeyboardTracking(false); y->setKeyboardTracking(false);
            x->setValue(xv); y->setValue(yv);
            table->setCellWidget(row, 0, x); table->setCellWidget(row, 1, y);
            connect(x, qOverload<double>(&QDoubleSpinBox::valueChanged), this, [=](double) { if (!*syncing) (*sync)(); });
            connect(y, qOverload<double>(&QDoubleSpinBox::valueChanged), this, [=](double) { if (!*syncing) (*sync)(); });
        };
        auto populate = [=](const std::vector<QPointF> &pts) {
            *syncing = true;
            table->setRowCount(int(pts.size()));
            for (int i = 0; i < int(pts.size()); ++i) addRow(i, pts[i].x(), pts[i].y());
            *syncing = false;
        };
        *sync = [=] {
            const std::vector<QPointF> pts = pointsFromTable();
            QJsonArray points;
            for (const QPointF &pt : pts) points.append(QJsonObject{{"x", pt.x()}, {"y", pt.y()}});
            channels->replace(channel->currentIndex(), points);
            adjustment->insert("curves", QJsonObject{{"channel", channel->currentText()}, {"channels", *channels}});
            graph->setPoints(pts);
            debounce->start();
        };
        auto reload = [=](int index) {
            std::vector<QPointF> pts;
            for (const QJsonValue &v : channels->at(index).toArray()) pts.push_back(QPointF(v.toObject().value("x").toDouble(), v.toObject().value("y").toDouble()));
            populate(pts);
            graph->setPoints(pts);
        };
        graph->onChange = [=](const std::vector<QPointF> &pts) { populate(pts); (*sync)(); };
        reload(channel->currentIndex());
        connect(channel, qOverload<int>(&QComboBox::currentIndexChanged), this, [=](int index) { reload(index); });
        connect(addBtn, &QPushButton::clicked, this, [=] {
            std::vector<QPointF> pts = pointsFromTable();
            pts.push_back(QPointF(255, 255));
            populate(pts); (*sync)();
        });
        connect(delBtn, &QPushButton::clicked, this, [=] {
            if (table->rowCount() <= 2) return;
            std::vector<QPointF> pts = pointsFromTable();
            pts.erase(pts.begin() + (table->currentRow() < 0 ? int(pts.size()) - 1 : table->currentRow()));
            populate(pts); (*sync)();
        });
        connect(resetBtn, &QPushButton::clicked, this, [=] {
            populate({QPointF(0, 0), QPointF(255, 255)}); (*sync)();
        });
    }
    if (kind == "Exposure") {
        auto *exposure = new QDoubleSpinBox(this), *offset = new QDoubleSpinBox(this), *gamma = new QDoubleSpinBox(this);
        auto addNum = [=](QDoubleSpinBox *box, const QString &label, double min, double max, double value, int decimals) {
            box->setObjectName(label); box->setAccessibleName(label);
            box->setRange(min, max); box->setDecimals(decimals); box->setValue(value); box->setKeyboardTracking(false);
            form->addRow(tr("&%1").arg(label), sliderRow(box, this));
            connect(box, qOverload<double>(&QDoubleSpinBox::valueChanged), this, [=](double) {
                adjustment->insert("exposureSettings", QJsonObject{{"exposure", exposure->value()}, {"offset", offset->value()}, {"gamma", gamma->value()}});
                debounce->start();
            });
        };
        addNum(exposure, "Exposure", -20, 20, 0, 2);
        addNum(offset, "Offset", -0.5, 0.5, 0, 4);
        addNum(gamma, "Gamma", 0.01, 9.99, 1, 2);
    }
    if (kind == "Gradient Map") {
        auto *shadows = new QPushButton(this), *highlights = new QPushButton(this);
        auto *reversed = new QCheckBox(tr("Reversed"), this); extras->addWidget(reversed);
        QColor shadowColor(0, 0, 0), highlightColor(255, 255, 255);
        auto paint = [=](QPushButton *btn, const QColor &c) {
            btn->setStyleSheet(QString("background-color: %1; border: 1px solid #555555;").arg(c.name()));
            btn->setMinimumHeight(24);
        };
        paint(shadows, shadowColor); paint(highlights, highlightColor);
        form->addRow(tr("&Shadows"), shadows); form->addRow(tr("&Highlights"), highlights);
        auto emitSettings = [=] {
            auto colorJson = [](const QColor &c) -> QJsonObject {
                return {{"red", c.redF()}, {"green", c.greenF()}, {"blue", c.blueF()}};
            };
            adjustment->insert("gradientMapSettings", QJsonObject{
                {"shadows", colorJson(shadowColor)}, {"highlights", colorJson(highlightColor)},
                {"reversed", reversed->isChecked()}});
            debounce->start();
        };
        auto pick = [=](QPushButton *btn, QColor *target) {
            connect(btn, &QPushButton::clicked, this, [=] {
                const QColor chosen = colors->pick(*target, btn->text());
                if (chosen.isValid()) { *target = chosen; paint(btn, chosen); emitSettings(); }
            });
        };
        pick(shadows, &shadowColor); pick(highlights, &highlightColor);
        connect(reversed, &QCheckBox::toggled, this, [=](bool) { emitSettings(); });
        emitSettings();
    }
    if (kind == "Grain") {
        auto *amount = new QDoubleSpinBox(this), *size = new QDoubleSpinBox(this), *roughness = new QDoubleSpinBox(this);
        auto addNum = [=](QDoubleSpinBox *box, const QString &label, double min, double max, double value, int decimals) {
            box->setObjectName(label); box->setAccessibleName(label);
            box->setRange(min, max); box->setDecimals(decimals); box->setValue(value); box->setKeyboardTracking(false);
            form->addRow(tr("&%1").arg(label), sliderRow(box, this));
            connect(box, qOverload<double>(&QDoubleSpinBox::valueChanged), this, [=](double) {
                adjustment->insert("grainSettings", QJsonObject{{"amount", amount->value()}, {"size", size->value()}, {"roughness", roughness->value()}, {"seed", 0}});
                debounce->start();
            });
        };
        addNum(amount, "Amount", 0, 100, 25, 0);
        addNum(size, "Size (px)", 0.5, 20, 2, 1);
        addNum(roughness, "Roughness", 0, 100, 50, 0);
    }

    auto update = [=] {
        const bool ok = submit({{"action", "adjustmentPreview"}, {"adjustment", *adjustment}});
        error->setText(ok ? QString() : tr("Preview failed. Adjust the settings or cancel."));
        buttons->button(QDialogButtonBox::Ok)->setEnabled(ok);
        return ok;
    };
    connect(debounce, &QTimer::timeout, this, [=] { update(); });
    connect(preview, &QCheckBox::toggled, this, [=](bool enabled) {
        if (enabled) debounce->start(); else debounce->stop();
    });
    connect(buttons, &QDialogButtonBox::accepted, this, [=] {
        debounce->stop();
        if (update() && submit({{"action", "adjustmentCommit"}, {"adjustment", *adjustment}})) accept();
        else error->setText(tr("Could not apply the adjustment. Adjust the settings or cancel."));
    });
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(this, &QDialog::rejected, this, [=] { debounce->stop(); submit({{"action", "adjustmentCancel"}}); });
    debounce->start(0);
}