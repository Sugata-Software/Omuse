#include "EditorDialogs.h"
#include "QtPlatformServices.h"

#include <QCheckBox>
#include <QColorDialog>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QLabel>
#include <QPushButton>
#include <QSignalBlocker>
#include <QVBoxLayout>
#include <cmath>
#include <memory>

namespace {
struct SizeDraft {
    double width, height, resolution;
    double originalWidth, originalHeight;
    int unit = 0;
    bool relative = false, locked = false, resample = true;
    QColor fill = Qt::white;

    double display(bool x) const {
        double value = (x ? width : height) - (relative ? (x ? originalWidth : originalHeight) : 0);
        if (unit == 1) return value * 100 / (x ? originalWidth : originalHeight);
        if (unit == 2) return value / resolution;
        if (unit == 3) return value / resolution * 2.54;
        return value;
    }
    void set(double value, bool x) {
        if (!resample) {
            if (value > 0) resolution = (x ? width : height) / value * (unit == 3 ? 2.54 : 1);
            return;
        }
        if (unit == 1) value *= (x ? originalWidth : originalHeight) / 100;
        if (unit == 2) value *= resolution;
        if (unit == 3) value *= resolution / 2.54;
        value += relative ? (x ? originalWidth : originalHeight) : 0;
        if (x) { width = value; if (locked) height = value * originalHeight / originalWidth; }
        else { height = value; if (locked) width = value * originalWidth / originalHeight; }
    }
    bool valid() const {
        return std::isfinite(width) && std::isfinite(height) && std::isfinite(resolution)
            && std::round(width) >= 1 && std::round(width) <= 30000
            && std::round(height) >= 1 && std::round(height) <= 30000
            && std::round(width) * std::round(height) <= 100000000
            && resolution >= 1 && resolution <= 9600;
    }
};
}

SizeDialog::SizeDialog(const QJsonObject &state, bool imageSize, QWidget *parent, std::shared_ptr<IColorPickerService> colors) : QDialog(parent) {
    if (!colors) colors = PlatformServices::qtDefaults().colors;
    setObjectName("sizeDialog");
    setWindowTitle(imageSize ? tr("Image Size") : tr("Canvas Size"));
    auto draft = std::make_shared<SizeDraft>();
    draft->width = draft->originalWidth = state.value("width").toInt();
    draft->height = draft->originalHeight = state.value("height").toInt();
    draft->resolution = state.value("resolution").toDouble(72);
    draft->locked = imageSize;
    auto *layout = new QVBoxLayout(this);
    layout->addWidget(new QLabel(tr("Current: %1 × %2 pixels").arg(draft->width).arg(draft->height), this));
    auto *form = new QFormLayout;
    layout->addLayout(form);
    auto *units = new QComboBox(this);
    units->setObjectName("units");
    units->addItems({tr("Pixels"), tr("Percent"), tr("Inches"), tr("Centimeters")});
    form->addRow(tr("&Units"), units);
    auto makeDimension = [this, form](const QString &label, const QString &name) {
        auto *field = new QDoubleSpinBox(this);
        field->setObjectName(name); field->setAccessibleName(label);
        field->setDecimals(3); field->setRange(-100000000, 100000000);
        field->setKeyboardTracking(false);
        form->addRow(label, field);
        return field;
    };
    auto *width = makeDimension(tr("&Width"), "width");
    auto *height = makeDimension(tr("&Height"), "height");
    auto *relative = new QCheckBox(tr("Relative to current dimensions"), this);
    relative->setObjectName("relative");
    if (!imageSize) layout->addWidget(relative); else relative->hide();
    auto *locked = new QCheckBox(tr("Lock aspect ratio"), this);
    locked->setObjectName("locked"); locked->setChecked(imageSize);
    layout->addWidget(locked);
    auto *resolution = new QDoubleSpinBox(this);
    resolution->setObjectName("resolution"); resolution->setDecimals(3);
    resolution->setRange(1, 9600); resolution->setSuffix(tr(" pixels/inch"));
    resolution->setKeyboardTracking(false);
    auto *resample = new QCheckBox(tr("Resample"), this);
    resample->setObjectName("resample"); resample->setChecked(true);
    auto *sampling = new QComboBox(this);
    sampling->setObjectName("sampling");
    sampling->addItem(tr("Nearest"), "Nearest"); sampling->addItem(tr("Smooth"), "Smooth");
    sampling->addItem(tr("High quality"), "High quality"); sampling->setCurrentIndex(2);
    if (imageSize) {
        form->addRow(tr("&Resolution"), resolution); layout->addWidget(resample);
        form->addRow(tr("&Sampling"), sampling);
    } else { resolution->hide(); resample->hide(); sampling->hide(); }
    auto *anchor = new QComboBox(this);
    anchor->setObjectName("anchor");
    anchor->addItems({tr("Top left"), tr("Top center"), tr("Top right"), tr("Middle left"), tr("Center"),
                      tr("Middle right"), tr("Bottom left"), tr("Bottom center"), tr("Bottom right")});
    anchor->setCurrentIndex(4);
    auto *extension = new QComboBox(this);
    extension->setObjectName("extension");
    extension->addItems({tr("Transparent"), tr("Black"), tr("White"), tr("Custom")});
    auto *color = new QPushButton(tr("Choose extension color…"), this);
    color->setEnabled(false);
    if (!imageSize) {
        form->addRow(tr("&Anchor"), anchor); form->addRow(tr("Canvas &extension"), extension);
        layout->addWidget(color);
        layout->addWidget(new QLabel(tr("Artwork is not scaled. Cropped content remains outside the canvas."), this));
    } else { anchor->hide(); extension->hide(); color->hide(); }
    auto *result = new QLabel(this);
    result->setWordWrap(true); result->setObjectName("sizeResult"); layout->addWidget(result);
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    layout->addWidget(buttons);
    auto refresh = [=] {
        QSignalBlocker bw(width), bh(height), br(resolution), bu(units);
        width->setValue(draft->display(true)); height->setValue(draft->display(false));
        resolution->setValue(draft->resolution); units->setCurrentIndex(draft->unit);
        buttons->button(QDialogButtonBox::Ok)->setEnabled(draft->valid());
        result->setText(draft->valid() ? tr("Result: %1 × %2 pixels").arg(std::round(draft->width)).arg(std::round(draft->height))
            : tr("Use 1–30,000 pixels per side, up to 100 megapixels, and 1–9,600 pixels/inch."));
    };
    connect(width, &QDoubleSpinBox::valueChanged, this, [=](double v) { draft->set(v, true); refresh(); });
    connect(height, &QDoubleSpinBox::valueChanged, this, [=](double v) { draft->set(v, false); refresh(); });
    connect(units, &QComboBox::currentIndexChanged, this, [=](int v) {
        draft->unit = !draft->resample && v < 2 ? 2 : v; refresh();
    });
    connect(relative, &QCheckBox::toggled, this, [=](bool v) { draft->relative = v; refresh(); });
    connect(locked, &QCheckBox::toggled, this, [=](bool v) {
        draft->locked = v;
        if (v) draft->height = draft->width * draft->originalHeight / draft->originalWidth;
        refresh();
    });
    connect(resolution, &QDoubleSpinBox::valueChanged, this, [=](double v) {
        if (draft->resample && draft->unit >= 2) {
            draft->width *= v / draft->resolution; draft->height *= v / draft->resolution;
        }
        draft->resolution = v; refresh();
    });
    connect(resample, &QCheckBox::toggled, this, [=](bool v) {
        draft->resample = v; locked->setEnabled(v); sampling->setEnabled(v);
        if (!v) {
            draft->width = draft->originalWidth; draft->height = draft->originalHeight;
            locked->setChecked(true); if (draft->unit < 2) draft->unit = 2;
        }
        refresh();
    });
    connect(extension, &QComboBox::currentIndexChanged, this, [=](int v) { color->setEnabled(v == 3); });
    connect(color, &QPushButton::clicked, this, [=] {
        const QColor selected = colors->pick(draft->fill, tr("Canvas extension"));
        if (selected.isValid()) draft->fill = selected;
    });
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    m_command = [=] {
        QJsonObject command{{"action", imageSize ? "resizeImage" : "resizeCanvas"},
            {"width", int(std::round(draft->width))}, {"height", int(std::round(draft->height))}};
        if (imageSize) { command.insert("value", draft->resolution); command.insert("kind", sampling->currentData().toString()); }
        else {
            const int choice = extension->currentIndex();
            const QColor fill = choice == 1 ? QColor(Qt::black) : choice == 2 ? QColor(Qt::white) : draft->fill;
            command.insert("enabled", choice != 0);
            command.insert("parameters", QJsonObject{{"anchor", anchor->currentIndex()},
                {"red", fill.redF()}, {"green", fill.greenF()}, {"blue", fill.blueF()}});
        }
        return command;
    };
    refresh();
}

QJsonObject SizeDialog::command() const { return m_command(); }
