#include <QRegularExpression>
#include <QMenu>
#include <QMouseEvent>
// SwiftUIQtRenderer — see SwiftUIQtRenderer.h. Fetches the resolved tree via compositor_session_render_tree
// (same size-query convention as compositor_session_state, see SessionWindow::sessionState) and walks it once,
// mapping each `RenderNode` kind to the matching Qt widget. Interactive widgets (Button, Toggle) wire their Qt
// signal straight to compositor_session_dispatch_swiftui_action, by the node's id — the same generic path for
// every panel, no per-panel Qt glue. This is intentionally the *only* place that knows the kind→widget mapping.

#include "SwiftUIQtRenderer.h"

#include <QBoxLayout>
#include <QCheckBox>
#include <QFont>
#include <QFrame>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QComboBox>
#include <QLineEdit>
#include <QPainter>
#include <QPaintEvent>
#include <QPushButton>
#include <QScrollArea>
#include <QIcon>
#include <QPainterPath>
#include <QProgressBar>
#include <QSlider>
#include <QVariant>
#include <QPointer>
#include <QTimer>
#include <QSignalBlocker>
#include <algorithm>

extern "C" {
int64_t compositor_session_render_tree(uint64_t handle, const char *panel, uint8_t *output, size_t capacity);
int32_t compositor_session_dispatch_swiftui_action(uint64_t handle, const char *panel, const char *node_id, const char *handler_key,
                                                   const uint8_t *payload, size_t payload_count);
int64_t compositor_session_render_swiftui_canvas(uint64_t handle, const char *panel, const char *node_id, size_t width, size_t height,
                                                 uint8_t *output, size_t capacity);
}

namespace {

/// A `Canvas` node's widget: its `paintEvent` asks Swift to draw into a same-sized `CGContext` (the existing
/// Skia-backed bridge) and blits the result — upstream's `Canvas { context, size in ... }` closure runs unmodified,
/// this is the only Linux-specific code involved, and it is generic across every `Canvas` in every panel.
class SwiftUICanvasWidget : public QWidget {
public:
    SwiftUICanvasWidget(uint64_t handle, QString panel, QString nodeID, QWidget *parent = nullptr)
        : QWidget(parent), m_handle(handle), m_panel(std::move(panel)), m_nodeID(std::move(nodeID)) {}

protected:
    void paintEvent(QPaintEvent *) override {
        const int w = qMax(1, width()), h = qMax(1, height());
        const size_t capacity = static_cast<size_t>(w) * static_cast<size_t>(h) * 4;
        QByteArray bytes(static_cast<qsizetype>(capacity), Qt::Uninitialized);
        const QByteArray panelUtf8 = m_panel.toUtf8(), nodeUtf8 = m_nodeID.toUtf8();
        const int64_t written = compositor_session_render_swiftui_canvas(
            m_handle, panelUtf8.constData(), nodeUtf8.constData(), static_cast<size_t>(w), static_cast<size_t>(h),
            reinterpret_cast<uint8_t *>(bytes.data()), capacity);
        if (written != static_cast<int64_t>(capacity)) return;
        const QImage image(reinterpret_cast<const uchar *>(bytes.constData()), w, h, w * 4, QImage::Format_RGBA8888_Premultiplied);
        QPainter painter(this);
        painter.drawImage(0, 0, image);
    }

private:
    uint64_t m_handle;
    QString m_panel, m_nodeID;
};

} // namespace

namespace {

QString linuxText(QString text) {
    text.replace(QRegularExpression("\\bOption\\b"), "Alt");
    text.replace(QRegularExpression("\\bCommand\\b"), "Ctrl");
    text.replace(QString::fromUtf8("⌘"), "Ctrl+");
    text.replace(QString::fromUtf8("⌥"), "Alt+");
    return text;
}

QJsonObject fetchTree(uint64_t handle, const QString &panel) {
    const QByteArray panelUtf8 = panel.toUtf8();
    const int64_t size = compositor_session_render_tree(handle, panelUtf8.constData(), nullptr, 0);
    if (size <= 0 || size > 4 * 1024 * 1024) return {};
    QByteArray bytes(static_cast<qsizetype>(size), Qt::Uninitialized);
    if (compositor_session_render_tree(handle, panelUtf8.constData(), reinterpret_cast<uint8_t *>(bytes.data()), bytes.size()) != size)
        return {};
    return QJsonDocument::fromJson(bytes).object();
}

struct ActionListener {
    QPointer<QObject> owner;
    std::function<void(uint64_t, const QString &)> callback;
};
std::vector<ActionListener> g_actionListeners;

/// Sends `payload` (a JSON fragment: `true`/`false`, empty for a no-argument action) to the handler `handlerKey`
/// recorded for `nodeID` in the tree `panel` last resolved — the exact mechanism
/// `Sources/LinuxBridge/SwiftUIBridge.swift`'s `compositor_session_dispatch_swiftui_action` implements.
void dispatch(uint64_t handle, const QString &panel, const QString &nodeID, const QString &handlerKey, const QByteArray &payload = {}) {
    const QByteArray panelUtf8 = panel.toUtf8(), nodeUtf8 = nodeID.toUtf8(), keyUtf8 = handlerKey.toUtf8();
    const auto result = compositor_session_dispatch_swiftui_action(handle, panelUtf8.constData(), nodeUtf8.constData(), keyUtf8.constData(),
                                               payload.isEmpty() ? nullptr : reinterpret_cast<const uint8_t *>(payload.constData()),
                                               static_cast<size_t>(payload.size()));
    if (result != 0) return;
    // Never rebuild a control from inside its signal. The context also cancels
    // pending notifications automatically when its window is destroyed.
    for (const auto &listener : g_actionListeners) {
        if (listener.owner) QTimer::singleShot(0, listener.owner, [callback = listener.callback, handle, panel] {
            callback(handle, panel);
        });
    }
}

/// A JSON fragment for a number/string payload — `QJsonDocument` only emits whole documents, so a single `QJsonValue`
/// is wrapped in a throwaway array and unwrapped textually; simpler than hand-escaping strings.
QByteArray jsonFragment(const QJsonValue &value) {
    QByteArray array = QJsonDocument(QJsonArray{value}).toJson(QJsonDocument::Compact);
    return array.mid(1, array.size() - 2); // strip the wrapping '[' ']'
}

QIcon renderToolVectorIcon(const QString &symbol, int size = 20, const QColor &color = QColor(0xf5, 0xf5, 0xf7)) {
    QPixmap pix(size, size);
    pix.fill(Qt::transparent);
    QPainter p(&pix);
    p.setRenderHint(QPainter::Antialiasing, true);

    const qreal s = size / 20.0;
    p.scale(s, s);

    if (symbol == "arrow.up.left.and.arrow.down.right" || symbol == "move") {
        p.setPen(QPen(color, 1.6, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        p.drawLine(10, 3, 10, 17);
        p.drawLine(3, 10, 17, 10);
        p.drawLine(7, 6, 10, 3); p.drawLine(13, 6, 10, 3);
        p.drawLine(7, 14, 10, 17); p.drawLine(13, 14, 10, 17);
        p.drawLine(6, 7, 3, 10); p.drawLine(6, 13, 3, 10);
        p.drawLine(14, 7, 17, 10); p.drawLine(14, 13, 17, 10);
    } else if (symbol.contains("circle.dashed")) {
        QPen dashPen(color, 1.5, Qt::CustomDashLine, Qt::SquareCap);
        dashPen.setDashPattern({2, 2});
        p.setPen(dashPen);
        p.drawEllipse(QRectF(3, 3, 14, 14));
    } else if (symbol.contains("rectangle.dashed") || symbol == "marquee") {
        QPen dashPen(color, 1.5, Qt::CustomDashLine, Qt::SquareCap);
        dashPen.setDashPattern({2, 2});
        p.setPen(dashPen);
        p.drawRoundedRect(QRectF(3, 3, 14, 14), 1, 1);
    } else if (symbol.contains("lasso")) {
        p.setPen(QPen(color, 1.5, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        QPainterPath path;
        path.moveTo(5, 8);
        path.cubicTo(5, 3, 16, 3, 16, 9);
        path.cubicTo(16, 15, 11, 16, 8, 14);
        path.lineTo(4, 17);
        p.drawPath(path);
        p.drawLine(6, 13, 9, 16);
    } else if (symbol.contains("wand")) {
        p.setPen(QPen(color, 1.6, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        p.drawLine(3, 17, 13, 7);
        p.drawLine(12, 6, 14, 8);
        p.setPen(QPen(color, 1.2, Qt::SolidLine, Qt::RoundCap));
        p.drawLine(16, 2, 16, 6); p.drawLine(14, 4, 18, 4);
        p.drawLine(8, 2, 8, 4); p.drawLine(7, 3, 9, 3);
        p.drawLine(17, 9, 17, 11); p.drawLine(16, 10, 18, 10);
    } else if (symbol == "crop") {
        p.setPen(QPen(color, 1.6, Qt::SolidLine, Qt::SquareCap, Qt::MiterJoin));
        p.drawLine(2, 6, 14, 6);
        p.drawLine(6, 2, 6, 14);
        p.drawLine(6, 14, 18, 14);
        p.drawLine(14, 6, 14, 18);
    } else if (symbol.contains("eraser")) {
        p.setPen(QPen(color, 1.5, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        p.save();
        p.translate(10, 10);
        p.rotate(-30);
        p.drawRoundedRect(QRectF(-6, -4, 12, 8), 2, 2);
        p.drawLine(-1, -4, -1, 4);
        p.restore();
    } else if (symbol.contains("paintbrush") || symbol == "brush") {
        p.setPen(QPen(color, 1.5, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        QPainterPath b;
        b.moveTo(4, 16);
        b.cubicTo(3, 13, 6, 10, 9, 9);
        b.lineTo(11, 11);
        b.cubicTo(10, 14, 7, 17, 4, 16);
        p.setBrush(color);
        p.drawPath(b);
        p.setBrush(Qt::NoBrush);
        p.drawLine(9, 9, 15, 3);
        p.drawLine(11, 11, 17, 5);
        p.drawLine(15, 3, 17, 5);
    } else if (symbol.contains("bandage") || symbol == "spotHealing") {
        p.setPen(QPen(color, 1.5, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        p.save();
        p.translate(10, 10);
        p.rotate(45);
        p.drawRoundedRect(QRectF(-4, -8, 8, 16), 3, 3);
        p.setPen(QPen(color, 1.5, Qt::SolidLine, Qt::RoundCap));
        p.drawPoint(-1, -1); p.drawPoint(1, 1);
        p.drawPoint(-1, 1); p.drawPoint(1, -1);
        p.restore();
    } else if (symbol.contains("seal") || symbol == "cloneStamp") {
        p.setPen(QPen(color, 1.5, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        p.drawEllipse(8, 2, 4, 4);
        p.drawLine(10, 6, 10, 11);
        p.drawRoundedRect(QRectF(4, 11, 12, 4), 1.5, 1.5);
        p.fillRect(QRectF(3, 15, 14, 2), color);
    } else if (symbol.contains("drop") || symbol == "smear" || symbol == "blur") {
        p.setPen(QPen(color, 1.5, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        QPainterPath drop;
        drop.moveTo(10, 3);
        drop.cubicTo(10, 3, 4, 10, 4, 13);
        drop.arcTo(QRectF(4, 7, 12, 12), 180, 180);
        drop.cubicTo(16, 10, 10, 3, 10, 3);
        p.drawPath(drop);
    } else if (symbol.contains("square.bottomhalf.filled") || symbol == "gradient") {
        p.setPen(QPen(color, 1.5, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        p.drawRoundedRect(QRectF(3, 3, 14, 14), 2, 2);
        p.fillRect(QRectF(3, 10, 14, 7), color);
    } else if (symbol.contains("square.on.circle") || symbol == "shape") {
        p.setPen(QPen(color, 1.5, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        p.drawEllipse(QRectF(7, 7, 10, 10));
        p.drawRoundedRect(QRectF(3, 3, 9, 9), 1, 1);
    } else if (symbol.contains("textformat") || symbol == "type") {
        p.setPen(QPen(color, 1.8, Qt::SolidLine, Qt::SquareCap, Qt::MiterJoin));
        p.drawLine(4, 4, 16, 4);
        p.drawLine(10, 4, 10, 16);
        p.drawLine(7, 16, 13, 16);
    } else if (symbol.contains("eyedropper")) {
        p.setPen(QPen(color, 1.5, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        p.drawLine(4, 16, 7, 13);
        p.drawLine(7, 13, 13, 7);
        p.drawLine(13, 7, 15, 9);
        p.drawLine(15, 9, 9, 15);
        p.drawLine(9, 15, 4, 16);
        p.drawLine(13, 7, 16, 4);
        p.drawEllipse(15, 3, 3, 3);
    } else if (symbol.contains("hand.draw") || symbol == "hand") {
        p.setPen(QPen(color, 1.5, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        p.drawRoundedRect(QRectF(5, 8, 9, 10), 2, 2);
        p.drawLine(7, 4, 7, 8);
        p.drawLine(9, 3, 9, 8);
        p.drawLine(11, 4, 11, 8);
        p.drawLine(13, 6, 13, 8);
        p.drawLine(4, 10, 4, 13);
    } else if (symbol.contains("magnifyingglass") || symbol == "zoom") {
        p.setPen(QPen(color, 1.6, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        p.drawEllipse(QRectF(4, 4, 9, 9));
        p.drawLine(11, 11, 16, 16);
    } else if (symbol.contains("arrow.left.and.right")) {
        p.setPen(QPen(color, 1.3, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        p.drawLine(3, 7, 13, 7);
        p.drawLine(11, 5, 13, 7); p.drawLine(11, 9, 13, 7);
        p.drawLine(13, 11, 3, 11);
        p.drawLine(5, 9, 3, 11); p.drawLine(5, 13, 3, 11);
    } else if (symbol.contains("arrow.counterclockwise")) {
        p.setPen(QPen(color, 1.3, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        p.drawArc(QRectF(3, 3, 10, 10), 45 * 16, 270 * 16);
        p.drawLine(10, 2, 10, 5); p.drawLine(10, 2, 13, 2);
    } else {
        p.setPen(QPen(color, 1.5, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        p.drawRect(4, 4, 12, 12);
    }
    p.end();
    return QIcon(pix);
}

/// Applies the modifiers this first pass understands (the highest-frequency ones, per the plan); an unrecognised
/// modifier kind is silently skipped rather than failing the whole render — additive coverage, not all-or-nothing.
void applyModifiers(QWidget *widget, const QJsonArray &modifiers) {
    for (const auto &entry : modifiers) {
        const QJsonObject modifier = entry.toObject();
        const QString kind = modifier.value("kind").toString();
        const QJsonObject doubles = modifier.value("doubleParams").toObject();
        const QJsonObject strings = modifier.value("stringParams").toObject();
        const QJsonObject bools = modifier.value("boolParams").toObject();
        if (kind == "frame") {
            if (doubles.contains("width")) widget->setFixedWidth(static_cast<int>(doubles.value("width").toDouble()));
            if (doubles.contains("height")) widget->setFixedHeight(static_cast<int>(doubles.value("height").toDouble()));
        } else if (kind == "padding") {
            if (auto *layout = widget->layout()) {
                layout->setContentsMargins(static_cast<int>(doubles.value("leading").toDouble()),
                                            static_cast<int>(doubles.value("top").toDouble()),
                                            static_cast<int>(doubles.value("trailing").toDouble()),
                                            static_cast<int>(doubles.value("bottom").toDouble()));
            }
        } else if (kind == "font") {
            const QString name = strings.value("name").toString();
            QFont font = widget->font();
            if (name.contains(QStringLiteral("bold"), Qt::CaseInsensitive)) font.setBold(true);
            widget->setFont(font);
        } else if (kind == "disabled") {
            widget->setDisabled(bools.value("value").toBool());
        } else if (kind == "offset") {
            const int ox = static_cast<int>(doubles.value("x").toDouble());
            const int oy = static_cast<int>(doubles.value("y").toDouble());
            widget->move(widget->x() + ox, widget->y() + oy);
            widget->setProperty("offsetX", ox);
            widget->setProperty("offsetY", oy);
        } else if (kind == "fixedSize") {
            widget->setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Fixed);
        } else if (kind == "help" || kind == "accessibilityLabel") {
            const QString text = linuxText(strings.value("text").toString());
            if (!text.isEmpty()) {
                if (kind == "accessibilityLabel") widget->setAccessibleName(text);
                else widget->setToolTip(text);
            }
        } else if (kind == "accessibilityIdentifier") {
            const QString id = strings.value("text").toString();
            if (!id.isEmpty()) widget->setObjectName(id);
        }
    }
}

// Filled shape labels carry actual RGB values, including NSColor-backed palette swatches.
QColor swatchColor(const QJsonObject &node) {
    const auto kind=node.value("kind").toString();
    if(kind=="Shape" || kind.contains("Rectangle") || kind=="Circle" || kind=="Ellipse") {
        for(auto v:node.value("modifiers").toArray()) {
            const auto m=v.toObject(); const auto name=m.value("stringParams").toObject().value("name").toString();
            if(m.value("kind")=="foregroundStyle" && (name.startsWith("rgb:") || name.startsWith("cgColor:"))) {
                const auto parts=name.mid(name.indexOf(':')+1).split(',');
                if(parts.size()==4) return QColor::fromRgbF(parts[0].toDouble(),parts[1].toDouble(),parts[2].toDouble(),parts[3].toDouble());
            }
        }
    }
    for(auto v:node.value("children").toArray()) {auto color=swatchColor(v.toObject()); if(color.isValid()) return color;}
    return {};
}
class TapFilter : public QObject {
public:
    TapFilter(QWidget *owner,std::function<void(bool)> callback):QObject(owner),fire(std::move(callback)) {owner->installEventFilter(this);}
    std::function<void(bool)> fire;
    bool eventFilter(QObject *,QEvent *e) override {
        if(e->type()==QEvent::MouseButtonRelease || e->type()==QEvent::MouseButtonDblClick) {
            auto *mouse=static_cast<QMouseEvent *>(e);
            if(mouse->button()==Qt::LeftButton) fire(e->type()==QEvent::MouseButtonDblClick);
        }
        return false;
    }
};
QString labelText(const QJsonObject &node) {
    if(node.value("kind")=="Text") return node.value("stringParams").toObject().value("text").toString();
    for(auto v:node.value("children").toArray()) {const auto text=labelText(v.toObject()); if(!text.isEmpty()) return text;}
    return {};
}
void addContextActions(QMenu *menu,const QJsonObject &node,uint64_t handle,const QString &panel) {
    if(node.value("kind")=="Button") {
        auto *action=menu->addAction(labelText(node));
        for(auto v:node.value("modifiers").toArray()) if(v.toObject().value("kind")=="disabled")
            action->setEnabled(!v.toObject().value("boolParams").toObject().value("value").toBool());
        QObject::connect(action,&QAction::triggered,menu,[handle,panel,id=node.value("id").toString()] {dispatch(handle,panel,id,"action");});
    } else if(node.value("kind")=="Divider") menu->addSeparator();
    else for(auto v:node.value("children").toArray()) addContextActions(menu,v.toObject(),handle,panel);
}

QWidget *buildNode(uint64_t handle, const QString &panel, const QJsonObject &node);

/// `VStack`/`HStack` share everything but the layout's orientation.
QWidget *buildStack(uint64_t handle, const QString &panel, const QJsonObject &node, QBoxLayout::Direction direction) {
    auto *container = new QWidget;
    auto *layout = new QBoxLayout(direction, container);
    layout->setContentsMargins(0, 0, 0, 0);
    const auto parameters = node.value("doubleParams").toObject();
    if (parameters.contains("spacing")) layout->setSpacing(qRound(parameters.value("spacing").toDouble()));
    for (const auto &childValue : node.value("children").toArray()) {
        if (QWidget *child = buildNode(handle, panel, childValue.toObject())) layout->addWidget(child);
    }
    return container;
}

QWidget *buildNode(uint64_t handle, const QString &panel, const QJsonObject &node) {
    const QString kind = node.value("kind").toString();
    const QString id = node.value("id").toString();
    const QJsonObject strings = node.value("stringParams").toObject();
    const QJsonObject bools = node.value("boolParams").toObject();
    const QJsonArray children = node.value("children").toArray();

    QWidget *widget = nullptr;
    if (kind == "VStack") {
        widget = buildStack(handle, panel, node, QBoxLayout::TopToBottom);
    } else if (kind == "HStack") {
        widget = buildStack(handle, panel, node, QBoxLayout::LeftToRight);
    } else if (kind == "ZStack") {
        auto *container = new QWidget;
        for (const auto &childValue : children) {
            if (QWidget *child = buildNode(handle, panel, childValue.toObject())) { child->setParent(container); child->show(); }
        }
        widget = container;
    } else if (kind == "Text") {
        auto *label = new QLabel(linuxText(strings.value("text").toString()));
        label->setStyleSheet("color: #f5f5f7;");
        widget = label;
    } else if (kind == "Button") {
        auto *button = new QPushButton;
        QString systemIcon;
        bool isToolButton = false;
        QString textLabel;

        std::function<void(const QJsonObject &)> extractLabel = [&](const QJsonObject &n) {
            const QString childKind = n.value("kind").toString();
            if(childKind=="ContextMenu") return;
            if (childKind == "Text") {
                if (textLabel.isEmpty()) textLabel = n.value("stringParams").toObject().value("text").toString();
            } else if (childKind == "Image") {
                const QString source = n.value("stringParams").toObject().value("source").toString();
                if (source.startsWith(QLatin1String("system:"))) {
                    systemIcon = source.mid(7);
                    button->setProperty("systemIcon", systemIcon);
                    isToolButton = true;
                }
            } else if (childKind == "Canvas") {
                isToolButton = true;
            } else {
                for (const auto &c : n.value("children").toArray()) extractLabel(c.toObject());
            }
        };
        for (const auto &child : children) extractLabel(child.toObject());

        // Infer icon for Canvas tool buttons or named tools:
        if (systemIcon.isEmpty()) {
            QString hint;
            for (const auto &m : node.value("modifiers").toArray()) {
                const QJsonObject mo = m.toObject();
                const QString mkind = mo.value("kind").toString();
                if (mkind == "help" || mkind == "accessibilityLabel") {
                    hint = mo.value("stringParams").toObject().value("text").toString();
                    if (!hint.isEmpty()) break;
                }
            }
            if (hint.contains(QLatin1String("Gradient"), Qt::CaseInsensitive)) systemIcon = QStringLiteral("square.bottomhalf.filled");
            else if (hint.contains(QLatin1String("Clone"), Qt::CaseInsensitive)) systemIcon = QStringLiteral("seal");
            else if (hint.contains(QLatin1String("Lasso"), Qt::CaseInsensitive)) systemIcon = QStringLiteral("lasso");
            else if (hint.contains(QLatin1String("Wand"), Qt::CaseInsensitive) || hint.contains(QLatin1String("Object"), Qt::CaseInsensitive)) systemIcon = QStringLiteral("wand.and.stars");
            else if (hint.contains(QLatin1String("Move"), Qt::CaseInsensitive)) systemIcon = QStringLiteral("arrow.up.left.and.arrow.down.right");
            else if (hint.contains(QLatin1String("Marquee"), Qt::CaseInsensitive)) systemIcon = QStringLiteral("rectangle.dashed");
            else if (hint.contains(QLatin1String("Crop"), Qt::CaseInsensitive)) systemIcon = QStringLiteral("crop");
            else if (hint.contains(QLatin1String("Brush"), Qt::CaseInsensitive)) systemIcon = QStringLiteral("paintbrush.pointed");
            else if (hint.contains(QLatin1String("Eraser"), Qt::CaseInsensitive)) systemIcon = QStringLiteral("eraser");
            else if (hint.contains(QLatin1String("Healing"), Qt::CaseInsensitive)) systemIcon = QStringLiteral("bandage");
            else if (hint.contains(QLatin1String("Smear"), Qt::CaseInsensitive) || hint.contains(QLatin1String("Blur"), Qt::CaseInsensitive)) systemIcon = QStringLiteral("drop");
            else if (hint.contains(QLatin1String("Shape"), Qt::CaseInsensitive)) systemIcon = QStringLiteral("square.on.circle");
            else if (hint.contains(QLatin1String("Type"), Qt::CaseInsensitive)) systemIcon = QStringLiteral("textformat");
            else if (hint.contains(QLatin1String("Eye"), Qt::CaseInsensitive)) systemIcon = QStringLiteral("eyedropper");
            else if (hint.contains(QLatin1String("Hand"), Qt::CaseInsensitive)) systemIcon = QStringLiteral("hand.draw");
            else if (hint.contains(QLatin1String("Zoom"), Qt::CaseInsensitive)) systemIcon = QStringLiteral("magnifyingglass");
            else if (hint.contains(QLatin1String("Swap"), Qt::CaseInsensitive)) systemIcon = QStringLiteral("arrow.left.and.right");
            else if (hint.contains(QLatin1String("Default colors"), Qt::CaseInsensitive)) systemIcon = QStringLiteral("arrow.counterclockwise");
            if (!systemIcon.isEmpty()) isToolButton = true;
        }

        const bool isSelected = bools.value("isSelected").toBool();

        if (isToolButton && !systemIcon.isEmpty()) {
            QIcon icon = renderToolVectorIcon(systemIcon, 20, QColor(0xf5, 0xf5, 0xf7));
            button->setIcon(icon);
            button->setIconSize(QSize(20, 20));
            button->setText(QString());
            button->setFixedSize(36, 36);
            button->setCursor(Qt::PointingHandCursor);
            if (isSelected) {
                button->setStyleSheet(
                    "QPushButton { background-color: rgba(255, 255, 255, 0.15); border: 1px solid rgba(255, 255, 255, 0.18); border-radius: 7px; padding: 0px; margin: 0px; } "
                    "QPushButton:hover { background-color: rgba(255, 255, 255, 0.22); }"
                );
            } else {
                button->setStyleSheet(
                    "QPushButton { background-color: transparent; border: 1px solid transparent; border-radius: 7px; padding: 0px; margin: 0px; } "
                    "QPushButton:hover { background-color: rgba(255, 255, 255, 0.08); border: 1px solid rgba(255, 255, 255, 0.10); }"
                );
            }
        } else {
            button->setText(textLabel);
            button->setStyleSheet(
                "QPushButton { background-color: #2a2a2d; color: #ffffff; border: 1px solid #38383c; border-radius: 4px; padding: 4px 10px; font-size: 11px; font-weight: 500; } "
                "QPushButton:hover { background-color: #35353a; color: #ffffff; border-color: #55555c; } "
                "QPushButton:pressed { background-color: #1f1f22; }"
            );
        }

        QObject::connect(button, &QPushButton::clicked, button, [handle, panel, id] {
            dispatch(handle, panel, id, QStringLiteral("action"));
        });
        widget = button;
    } else if (kind == "Toggle") {
        auto *checkBox = new QCheckBox;
        if (!children.isEmpty()) {
            const QJsonObject label = children.first().toObject();
            if (label.value("kind").toString() == "Text") checkBox->setText(label.value("stringParams").toObject().value("text").toString());
        }
        checkBox->setChecked(bools.value("isOn").toBool());
        checkBox->setStyleSheet("QCheckBox { color: #f5f5f7; font-size: 11px; spacing: 6px; } QCheckBox::indicator { width: 14px; height: 14px; border: 1px solid #4a4a50; border-radius: 3px; background-color: #28282b; } QCheckBox::indicator:checked { background-color: #007aff; border-color: #007aff; }");
        QObject::connect(checkBox, &QCheckBox::toggled, checkBox, [handle, panel, id](bool checked) {
            dispatch(handle, panel, id, QStringLiteral("isOn"), checked ? "true" : "false");
        });
        widget = checkBox;
    } else if (kind == "TextField") {
        auto *field = new QLineEdit;
        field->setStyleSheet("QLineEdit { background-color: #28282b; color: #ffffff; border: 1px solid #444448; border-radius: 4px; padding: 2px 4px; font-size: 11px; } QLineEdit:focus { border-color: #007aff; }");
        const QStringList handlerKeys = [&] {
            QStringList keys;
            for (const auto &k : node.value("handlerKeys").toArray()) keys << k.toString();
            return keys;
        }();
        if (handlerKeys.contains(QStringLiteral("value"))) {
            const QJsonObject doubles = node.value("doubleParams").toObject();
            field->setText(QString::number(doubles.value("value").toDouble()));
            QObject::connect(field, &QLineEdit::textChanged, field, [handle, panel, id](const QString &text) {
                bool ok = false;
                const double number = text.toDouble(&ok);
                if (ok) dispatch(handle, panel, id, QStringLiteral("value"), jsonFragment(number));
            });
        } else {
            field->setText(strings.value("text").toString());
            if (handlerKeys.contains(QStringLiteral("text"))) {
                QObject::connect(field, &QLineEdit::textChanged, field, [handle, panel, id](const QString &text) {
                    dispatch(handle, panel, id, QStringLiteral("text"), jsonFragment(text));
                });
            }
        }
        widget = field;
    } else if (kind == "Slider") {
        const QJsonObject doubles = node.value("doubleParams").toObject();
        const double lower = doubles.value("lowerBound").toDouble(), upper = doubles.value("upperBound").toDouble(1);
        constexpr int steps = 1000;
        auto *slider = new QSlider(Qt::Horizontal);
        slider->setStyleSheet("QSlider::groove:horizontal { height: 4px; background: #333336; border-radius: 2px; } QSlider::sub-page:horizontal { background: #007aff; border-radius: 2px; } QSlider::handle:horizontal { background: #ffffff; border: 1px solid #b0b0b5; width: 12px; height: 12px; margin: -4px 0; border-radius: 6px; }");
        slider->setRange(0, steps);
        const double span = (upper > lower) ? (upper - lower) : 1;
        slider->setValue(static_cast<int>((doubles.value("value").toDouble() - lower) / span * steps));
        QObject::connect(slider, &QSlider::valueChanged, slider, [handle, panel, id, lower, span](int step) {
            dispatch(handle, panel, id, QStringLiteral("value"), jsonFragment(lower + span * step / 1000.0));
        });
        widget = slider;
    } else if (kind == "Picker") {
        auto *combo = new QComboBox;
        combo->setStyleSheet("QComboBox { background-color: #28282b; color: #ffffff; border: 1px solid #444448; border-radius: 4px; padding: 3px 8px; min-height: 18px; font-size: 11px; } QComboBox QAbstractItemView { background-color: #242427; color: #ffffff; selection-background-color: #007aff; }");
        const QString selection = strings.value("selection").toString();
        for (int i = 1; i < children.size(); ++i) {
            const QJsonObject item = children[i].toObject();
            combo->addItem(item.value("stringParams").toObject().value("text").toString());
            if(item.value("stringParams").toObject().value("text").toString()==selection) combo->setCurrentIndex(combo->count()-1);
            for (const auto &modifierValue : item.value("modifiers").toArray()) {
                const QJsonObject modifier = modifierValue.toObject();
                if (modifier.value("kind").toString() == "tag" &&
                    modifier.value("stringParams").toObject().value("text").toString() == selection) {
                    combo->setCurrentIndex(combo->count() - 1);
                }
            }
        }
        QObject::connect(combo, &QComboBox::currentIndexChanged, combo, [handle, panel, id](int index) {
            dispatch(handle, panel, id, QStringLiteral("selection"), jsonFragment(index));
        });
        widget = combo;
    } else if (kind == "ScrollView") {
        auto *scrollArea = new QScrollArea;
        scrollArea->setWidgetResizable(true);
        scrollArea->setFrameShape(QFrame::NoFrame);
        scrollArea->setStyleSheet("QScrollArea { background: transparent; border: none; } QScrollArea > QWidget > QWidget { background: transparent; }");
        scrollArea->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
        scrollArea->setVerticalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
        if (!children.isEmpty()) {
            if (QWidget *content = buildNode(handle, panel, children.first().toObject())) scrollArea->setWidget(content);
        }
        widget = scrollArea;
    } else if (kind == "Spacer") {
        auto *spacer = new QWidget;
        spacer->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
        widget = spacer;
    } else if (kind == "Divider") {
        auto *line = new QFrame;
        line->setFrameShape(QFrame::HLine);
        widget = line;
    } else if (kind == "Canvas") {
        widget = new SwiftUICanvasWidget(handle, panel, id);
    } else if (kind == "ProgressView") {
        auto *progress = new QProgressBar;
        progress->setRange(0, 0);
        progress->setTextVisible(false);
        progress->setFixedHeight(14);
        progress->setFixedWidth(14);
        widget = progress;
    } else {
        // Not yet mapped (e.g. _Native/LinearGradient land in later phases) — skip, don't crash.
        return nullptr;
    }

    widget->setProperty("swiftuiNodeID", id);
    if(kind=="Button") {
        const auto color=swatchColor(node);
        if(color.isValid()) {
            auto *button=static_cast<QPushButton *>(widget); button->setIcon({}); button->setText({});
            button->setFixedSize(36,24); button->setStyleSheet(QString("background:%1;border:1px solid #777;border-radius:3px;").arg(color.name()));
        }
    }
    if(kind=="TextField" || kind=="Picker") widget->setAccessibleName(strings.value("label").toString(labelText(node)));
    const auto keys=node.value("handlerKeys").toArray();
    if(keys.contains("tap") || keys.contains("doubleTap")) new TapFilter(widget,[handle,panel,id,keys](bool twice) {
        const auto key=twice ? "doubleTap" : "tap"; if(keys.contains(key)) dispatch(handle,panel,id,key);
    });
    if(keys.contains("submit")) {
        auto fields=widget->findChildren<QLineEdit *>(); if(auto *field=qobject_cast<QLineEdit *>(widget)) fields.append(field);
        for(auto *field:fields) QObject::connect(field,&QLineEdit::returnPressed,widget,[handle,panel,id,field] {dispatch(handle,panel,id,"submit"); field->clearFocus();});
    }
    for(auto child:children) if(child.toObject().value("kind")=="ContextMenu") {
        widget->setContextMenuPolicy(Qt::CustomContextMenu);
        QObject::connect(widget,&QWidget::customContextMenuRequested,widget,[widget,menuNode=child.toObject(),handle,panel](QPoint pos) {
            QMenu menu(widget); addContextActions(&menu,menuNode,handle,panel); menu.exec(widget->mapToGlobal(pos));
        });
    }
    applyModifiers(widget, node.value("modifiers").toArray());
    return widget;
}

// Reuse a structurally identical panel so typing and slider drags retain focus,
// selection, mouse capture and their Qt signal senders.
QJsonObject structure(QJsonObject node) {
    QJsonArray children;
    for (const auto &child : node.value("children").toArray()) children.append(structure(child.toObject()));
    return {{"id", node.value("id")}, {"kind", node.value("kind")},
            {"modifiers", node.value("modifiers")}, {"children", children},
            {"handlers", node.value("handlerKeys")}};
}
void updateValues(QWidget *root, const QJsonObject &node) {
    const QString id = node.value("id").toString();
    QWidget *widget = root->property("swiftuiNodeID").toString() == id ? root : nullptr;
    if (!widget) for (auto *child : root->findChildren<QWidget *>()) {
        if (child->property("swiftuiNodeID").toString() == id) { widget = child; break; }
    }
    if (widget) {
        const QSignalBlocker blocker(widget);
        const auto strings = node.value("stringParams").toObject();
        if(node.value("kind")=="Button") {const auto color=swatchColor(node); if(color.isValid()) widget->setStyleSheet(QString("background:%1;border:1px solid #777;border-radius:3px;").arg(color.name()));}
        const auto doubles = node.value("doubleParams").toObject();
        if (auto *field = qobject_cast<QLineEdit *>(widget)) {
            if (!field->hasFocus()) field->setText(doubles.contains("value")
                ? QString::number(doubles.value("value").toDouble()) : strings.value("text").toString());
        } else if (auto *slider = qobject_cast<QSlider *>(widget)) {
            const double lower = doubles.value("lowerBound").toDouble();
            const double span = doubles.value("upperBound").toDouble(1) - lower;
            if (!slider->isSliderDown() && span > 0)
                slider->setValue(qRound((doubles.value("value").toDouble() - lower) / span * 1000));
        } else if (auto *check = qobject_cast<QCheckBox *>(widget)) {
            check->setChecked(node.value("boolParams").toObject().value("isOn").toBool());
        } else if (auto *label = qobject_cast<QLabel *>(widget)) {
            if (strings.contains("text")) label->setText(linuxText(strings.value("text").toString()));
        } else if (auto *combo = qobject_cast<QComboBox *>(widget)) {
            const auto children = node.value("children").toArray();
            for(int i=1;i<children.size();++i) if(children[i].toObject().value("stringParams").toObject().value("text")==strings.value("selection")) combo->setCurrentIndex(i-1);
            for (int i = 1; i < children.size(); ++i) for (const auto &mod : children[i].toObject().value("modifiers").toArray()) {
                if (mod.toObject().value("kind") == "tag" && mod.toObject().value("stringParams").toObject().value("text") == strings.value("selection"))
                    combo->setCurrentIndex(i - 1);
            }
        }
    }
    for (const auto &child : node.value("children").toArray()) updateValues(root, child.toObject());
}

} // namespace

QWidget *swiftUIRenderPanel(uint64_t sessionHandle, const QString &panel, QWidget *existing) {
    const QJsonObject root = fetchTree(sessionHandle, panel);
    if (root.isEmpty()) return nullptr;
    const QByteArray shape = QJsonDocument(structure(root)).toJson(QJsonDocument::Compact);
    if (existing && existing->property("swiftuiSession").toULongLong() == sessionHandle
        && existing->property("swiftuiPanel").toString() == panel
        && existing->property("swiftuiStructure").toByteArray() == shape) {
        updateValues(existing, root);
        return existing;
    }
    auto *widget = buildNode(sessionHandle, panel, root);
    if (widget) {
        widget->setProperty("swiftuiStructure", shape);
        widget->setProperty("swiftuiSession", QVariant::fromValue<qulonglong>(sessionHandle));
        widget->setProperty("swiftuiPanel", panel);
    }
    return widget;
}

void registerSwiftUIActionListener(QObject *owner, std::function<void(uint64_t, const QString &)> listener) {
    if (!owner) return;
    g_actionListeners.push_back({owner, std::move(listener)});
    QObject::connect(owner, &QObject::destroyed, [] {
        g_actionListeners.erase(std::remove_if(g_actionListeners.begin(), g_actionListeners.end(),
            [](const ActionListener &listener) { return listener.owner.isNull(); }), g_actionListeners.end());
    });
}
